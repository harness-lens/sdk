// SPDX-License-Identifier: MPL-2.0
// Copyright © 2026 Cristian Camargo Filho

//! Local provider detection and installation boundaries.
//!
//! Catalog metadata is compiled into this adapter. Optional provider execution
//! requires explicit selection, a trusted non-virtual workspace, and a non-off
//! runtime mode. Installation additionally requires confirmation of the exact
//! locally generated plan. No shell is used and no provider output is returned.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use harness_lens_core::RuntimeMode;
pub use harness_lens_core::providers::*;

/// CodeBurn version covered by this adapter's contract tests.
pub const TESTED_CODEBURN_VERSION: &str = "0.9.24";
/// Maximum time allowed for local provider detection.
pub const DETECTION_TIMEOUT: Duration = Duration::from_secs(2);
/// Maximum time allowed for an explicitly confirmed package installation.
pub const INSTALLATION_TIMEOUT: Duration = Duration::from_secs(300);

const MAX_VERSION_OUTPUT_BYTES: u64 = 4_096;

/// Safe local inputs used to build provider status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderContext {
    /// Explicitly selected optional provider IDs. Native is always selected.
    pub selected: BTreeSet<String>,
    /// Runtime collection mode.
    pub runtime_mode: RuntimeMode,
    /// Whether snapshot mode has a configured local path.
    pub snapshot_configured: bool,
    /// Whether the workspace is trusted.
    pub workspace_trusted: bool,
    /// Whether the workspace uses a virtual filesystem.
    pub virtual_workspace: bool,
    /// Explicit executable path or command name used only in live mode.
    pub codeburn_executable: PathBuf,
}

impl Default for ProviderContext {
    fn default() -> Self {
        Self {
            selected: BTreeSet::new(),
            runtime_mode: RuntimeMode::Off,
            snapshot_configured: false,
            workspace_trusted: false,
            virtual_workspace: false,
            codeburn_executable: PathBuf::from("codeburn"),
        }
    }
}

/// Explicit confirmation inputs for the one allowlisted installation adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallationRequest {
    /// Provider selected by the user.
    pub provider_id: String,
    /// Exact tested version shown in the installation preview.
    pub requested_version: String,
    /// True only after the host confirms the exact generated preview.
    pub confirmed: bool,
    /// Trust state rechecked immediately before execution.
    pub workspace_trusted: bool,
    /// Virtual workspaces cannot run installers.
    pub virtual_workspace: bool,
}

/// Injectable process boundary. Implementations return only safe classifications
/// and normalized version strings, never stdout, stderr, arguments, or paths.
pub trait ProviderRuntime {
    /// Detects the local CodeBurn version within `timeout`.
    fn detect_codeburn_version(
        &self,
        executable: &Path,
        timeout: Duration,
    ) -> Result<String, ProviderError>;

    /// Installs the exact allowlisted CodeBurn version within `timeout`.
    fn install_codeburn(&self, version: &str, timeout: Duration) -> Result<(), ProviderError>;
}

/// Operating-system process boundary used by native hosts.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProviderRuntime;

impl ProviderRuntime for SystemProviderRuntime {
    fn detect_codeburn_version(
        &self,
        executable: &Path,
        timeout: Duration,
    ) -> Result<String, ProviderError> {
        let output = run_bounded(executable, &[OsString::from("--version")], timeout, true)?;
        parse_codeburn_version(&output).ok_or(ProviderError::InvalidVersion)
    }

    fn install_codeburn(&self, version: &str, timeout: Duration) -> Result<(), ProviderError> {
        if version != TESTED_CODEBURN_VERSION {
            return Err(ProviderError::InvalidVersion);
        }
        run_bounded(
            Path::new("npm"),
            &[
                OsString::from("install"),
                OsString::from("--global"),
                OsString::from(format!("codeburn@{version}")),
            ],
            timeout,
            false,
        )?;
        Ok(())
    }
}

/// Provider catalog and allowlisted installation service.
pub struct ProviderService<R = SystemProviderRuntime> {
    runtime: R,
}

impl Default for ProviderService<SystemProviderRuntime> {
    fn default() -> Self {
        Self {
            runtime: SystemProviderRuntime,
        }
    }
}

impl<R: ProviderRuntime> ProviderService<R> {
    /// Creates a service around an injectable process boundary.
    pub fn new(runtime: R) -> Self {
        Self { runtime }
    }

    /// Builds a deterministic catalog. Optional detection runs only for an
    /// explicitly selected provider in trusted, non-virtual live mode.
    pub fn catalog(&self, context: &ProviderContext) -> Result<Vec<ProviderStatus>, ProviderError> {
        let allowed = [NATIVE_ID.to_owned(), CODEBURN_ID.to_owned()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        if !context.selected.is_subset(&allowed) {
            return Err(ProviderError::InvalidProvider);
        }

        let mut catalog = vec![native_status(), self.codeburn_status(context)];
        catalog.sort_by(|left, right| left.descriptor.id.cmp(&right.descriptor.id));
        Ok(catalog)
    }

    /// Returns a fixed local preview. The preview itself grants no execution
    /// authority and cannot be deserialized into a runnable plan.
    pub fn installation_plan(
        &self,
        provider_id: &str,
        requested_version: Option<&str>,
    ) -> Result<InstallationPlan, ProviderError> {
        if provider_id != CODEBURN_ID {
            return Err(ProviderError::UnsupportedInstallation);
        }
        let version = requested_version.unwrap_or(TESTED_CODEBURN_VERSION);
        if version != TESTED_CODEBURN_VERSION {
            return Err(ProviderError::InvalidVersion);
        }
        Ok(codeburn_installation_plan(version))
    }

    /// Executes the fixed npm adapter after exact-version consent and a fresh
    /// workspace policy check. Caller-supplied command words are never accepted.
    pub fn install(&self, request: &InstallationRequest) -> Result<(), ProviderError> {
        if request.provider_id != CODEBURN_ID {
            return Err(ProviderError::UnsupportedInstallation);
        }
        if !request.confirmed {
            return Err(ProviderError::ConsentRequired);
        }
        if !request.workspace_trusted || request.virtual_workspace {
            return Err(ProviderError::WorkspaceBlocked);
        }
        if request.requested_version != TESTED_CODEBURN_VERSION {
            return Err(ProviderError::InvalidVersion);
        }
        self.runtime
            .install_codeburn(TESTED_CODEBURN_VERSION, INSTALLATION_TIMEOUT)
    }

    fn codeburn_status(&self, context: &ProviderContext) -> ProviderStatus {
        let selected = context.selected.contains(CODEBURN_ID);
        let mut status = ProviderStatus {
            descriptor: codeburn_descriptor(None),
            selected,
            availability: ProviderAvailability::Off,
            installation: InstallationStatus::Unknown,
            refresh: RefreshState {
                generation: 0,
                last_success: None,
                health: ProviderHealth::Disabled,
                error: None,
            },
        };
        if !selected || context.runtime_mode == RuntimeMode::Off {
            return status;
        }
        if !context.workspace_trusted || context.virtual_workspace {
            status.availability = ProviderAvailability::Blocked;
            status.refresh.error = Some(ProviderError::WorkspaceBlocked);
            return status;
        }
        if context.runtime_mode == RuntimeMode::Snapshot {
            if context.snapshot_configured {
                status.availability = ProviderAvailability::Available;
                status.refresh.health = ProviderHealth::NeverRefreshed;
            } else {
                status.availability = ProviderAvailability::ConfigurationError;
                status.refresh.health = ProviderHealth::Failed;
                status.refresh.error = Some(ProviderError::ConfigurationError);
            }
            return status;
        }

        match self
            .runtime
            .detect_codeburn_version(&context.codeburn_executable, DETECTION_TIMEOUT)
        {
            Ok(version) if version == TESTED_CODEBURN_VERSION => {
                status.descriptor = codeburn_descriptor(Some(version));
                status.availability = ProviderAvailability::Available;
                status.installation = InstallationStatus::Installed;
                status.refresh.health = ProviderHealth::NeverRefreshed;
            }
            Ok(_) | Err(ProviderError::InvalidVersion) => {
                status.availability = ProviderAvailability::InvalidVersion;
                status.installation = InstallationStatus::Installed;
                status.refresh.health = ProviderHealth::Failed;
                status.refresh.error = Some(ProviderError::InvalidVersion);
            }
            Err(error) => {
                status.availability = availability_for_error(error);
                status.installation = if error == ProviderError::NotFound {
                    InstallationStatus::NotInstalled
                } else {
                    InstallationStatus::Unknown
                };
                status.refresh.health = ProviderHealth::Failed;
                status.refresh.error = Some(error);
            }
        }
        status
    }
}

fn native_status() -> ProviderStatus {
    ProviderStatus {
        descriptor: ProviderDescriptor {
            id: NATIVE_ID.to_owned(),
            display_name: "Harness Lens Native".to_owned(),
            version: None,
            license: Some("MPL-2.0".to_owned()),
            source_url: "https://github.com/harness-lens/core".to_owned(),
            capabilities: vec![
                ProviderCapability::DeterministicAnalysis,
                ProviderCapability::LexicalSimilarity,
            ],
            configuration: vec![],
            platforms: all_platforms(),
            methods: vec![
                harness_lens_core::ScoreMethod::Deterministic,
                harness_lens_core::ScoreMethod::Heuristic,
            ],
            optional: false,
        },
        selected: true,
        availability: ProviderAvailability::Available,
        installation: InstallationStatus::BuiltIn,
        refresh: RefreshState {
            generation: 0,
            last_success: Some(0),
            health: ProviderHealth::Healthy,
            error: None,
        },
    }
}

fn codeburn_descriptor(version: Option<String>) -> ProviderDescriptor {
    ProviderDescriptor {
        id: CODEBURN_ID.to_owned(),
        display_name: "CodeBurn".to_owned(),
        version,
        license: Some("MIT".to_owned()),
        source_url: "https://github.com/getagentseal/codeburn".to_owned(),
        capabilities: vec![
            ProviderCapability::RuntimeAggregates,
            ProviderCapability::Snapshots,
            ProviderCapability::InstallationPlanning,
        ],
        configuration: vec![
            ConfigurationRequirement::RuntimeMode,
            ConfigurationRequirement::Executable,
            ConfigurationRequirement::SnapshotPath,
        ],
        platforms: all_platforms(),
        methods: vec![
            harness_lens_core::ScoreMethod::Heuristic,
            harness_lens_core::ScoreMethod::Statistical,
        ],
        optional: true,
    }
}

fn all_platforms() -> Vec<ProviderPlatform> {
    vec![
        ProviderPlatform::Linux,
        ProviderPlatform::Macos,
        ProviderPlatform::Windows,
    ]
}

fn availability_for_error(error: ProviderError) -> ProviderAvailability {
    match error {
        ProviderError::NotFound => ProviderAvailability::NotFound,
        ProviderError::InvalidVersion => ProviderAvailability::InvalidVersion,
        ProviderError::ConfigurationError => ProviderAvailability::ConfigurationError,
        ProviderError::WorkspaceBlocked => ProviderAvailability::Blocked,
        ProviderError::RuntimeFailure | ProviderError::Timeout => {
            ProviderAvailability::RuntimeFailure
        }
        ProviderError::InvalidProvider
        | ProviderError::LimitExceeded
        | ProviderError::InvalidReport
        | ProviderError::UnsupportedInstallation
        | ProviderError::ConsentRequired => ProviderAvailability::RuntimeFailure,
    }
}

fn codeburn_installation_plan(version: &str) -> InstallationPlan {
    InstallationPlan {
        provider_id: CODEBURN_ID.to_owned(),
        package_source: PackageSource {
            registry: "npm".to_owned(),
            package: "codeburn".to_owned(),
            url: "https://www.npmjs.com/package/codeburn".to_owned(),
        },
        requested_version: Some(version.to_owned()),
        command_preview: vec![
            "npm".to_owned(),
            "install".to_owned(),
            "--global".to_owned(),
            format!("codeburn@{version}"),
        ],
        expected_executable: "codeburn".to_owned(),
        license_url: "https://github.com/getagentseal/codeburn/blob/main/LICENSE".to_owned(),
        network_effects: vec![
            NetworkEffect::RegistryDownload,
            NetworkEffect::PackageScripts,
        ],
        filesystem_effects: vec![
            FilesystemEffect::PackageCache,
            FilesystemEffect::UserInstallation,
            FilesystemEffect::PackageScripts,
        ],
        restart_required: true,
    }
}

fn parse_codeburn_version(output: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(output).ok()?;
    text.split_whitespace().find_map(|word| {
        let candidate = word
            .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '.')
            .strip_prefix('v')
            .unwrap_or_else(|| {
                word.trim_matches(|character: char| {
                    !character.is_ascii_alphanumeric() && character != '.'
                })
            });
        let mut parts = candidate.split('.');
        let valid = parts.by_ref().take(3).all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        }) && parts.next().is_none()
            && candidate.matches('.').count() == 2;
        valid.then(|| candidate.to_owned())
    })
}

fn run_bounded(
    executable: &Path,
    arguments: &[OsString],
    timeout: Duration,
    capture_stdout: bool,
) -> Result<Vec<u8>, ProviderError> {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(if capture_stdout {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ProviderError::NotFound
        } else {
            ProviderError::RuntimeFailure
        }
    })?;
    let reader = child.stdout.take().map(|stdout| {
        thread::spawn(move || {
            let mut output = Vec::new();
            stdout
                .take(MAX_VERSION_OUTPUT_BYTES + 1)
                .read_to_end(&mut output)
                .map(|_| output)
        })
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProviderError::Timeout);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProviderError::RuntimeFailure);
            }
        }
    };
    let output = match reader {
        Some(reader) => reader
            .join()
            .map_err(|_| ProviderError::RuntimeFailure)?
            .map_err(|_| ProviderError::RuntimeFailure)?,
        None => Vec::new(),
    };
    if !status.success() {
        return Err(ProviderError::RuntimeFailure);
    }
    if output.len() as u64 > MAX_VERSION_OUTPUT_BYTES {
        return Err(ProviderError::LimitExceeded);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct FakeRuntime {
        detection: Result<String, ProviderError>,
        detection_calls: Cell<usize>,
        installs: RefCell<Vec<String>>,
    }

    impl FakeRuntime {
        fn new(detection: Result<&str, ProviderError>) -> Self {
            Self {
                detection: detection.map(str::to_owned),
                detection_calls: Cell::new(0),
                installs: RefCell::new(Vec::new()),
            }
        }
    }

    impl ProviderRuntime for FakeRuntime {
        fn detect_codeburn_version(
            &self,
            _executable: &Path,
            _timeout: Duration,
        ) -> Result<String, ProviderError> {
            self.detection_calls.set(self.detection_calls.get() + 1);
            self.detection.clone()
        }

        fn install_codeburn(&self, version: &str, _timeout: Duration) -> Result<(), ProviderError> {
            self.installs.borrow_mut().push(version.to_owned());
            Ok(())
        }
    }

    fn selected_context(mode: RuntimeMode) -> ProviderContext {
        ProviderContext {
            selected: [CODEBURN_ID.to_owned()].into(),
            runtime_mode: mode,
            snapshot_configured: false,
            workspace_trusted: true,
            virtual_workspace: false,
            codeburn_executable: PathBuf::from("codeburn"),
        }
    }

    #[test]
    fn off_is_native_only_and_runs_no_detection() {
        let runtime = FakeRuntime::new(Ok(TESTED_CODEBURN_VERSION));
        let service = ProviderService::new(runtime);
        let mut context = ProviderContext::default();
        context.selected.insert(CODEBURN_ID.to_owned());
        let catalog = service.catalog(&context).unwrap();
        assert_eq!(
            catalog
                .iter()
                .map(|status| status.descriptor.id.as_str())
                .collect::<Vec<_>>(),
            [CODEBURN_ID, NATIVE_ID]
        );
        assert!(catalog[0].selected);
        assert_eq!(catalog[0].availability, ProviderAvailability::Off);
        assert!(catalog[1].selected);
        assert_eq!(catalog[1].installation, InstallationStatus::BuiltIn);
        assert_eq!(service.runtime.detection_calls.get(), 0);
    }

    #[test]
    fn live_detection_is_bounded_to_selected_trusted_workspaces() {
        let service = ProviderService::new(FakeRuntime::new(Ok(TESTED_CODEBURN_VERSION)));
        let catalog = service
            .catalog(&selected_context(RuntimeMode::Live))
            .unwrap();
        assert_eq!(catalog[0].availability, ProviderAvailability::Available);
        assert_eq!(catalog[0].installation, InstallationStatus::Installed);
        assert_eq!(catalog[0].descriptor.version.as_deref(), Some("0.9.24"));
        assert_eq!(service.runtime.detection_calls.get(), 1);

        let service = ProviderService::new(FakeRuntime::new(Ok(TESTED_CODEBURN_VERSION)));
        let mut blocked = selected_context(RuntimeMode::Live);
        blocked.workspace_trusted = false;
        let catalog = service.catalog(&blocked).unwrap();
        assert_eq!(catalog[0].availability, ProviderAvailability::Blocked);
        assert_eq!(service.runtime.detection_calls.get(), 0);
    }

    #[test]
    fn detection_exposes_only_safe_status_and_tested_version() {
        let service = ProviderService::new(FakeRuntime::new(Err(ProviderError::NotFound)));
        let status = service
            .catalog(&selected_context(RuntimeMode::Live))
            .unwrap()
            .remove(0);
        assert_eq!(status.availability, ProviderAvailability::NotFound);
        assert_eq!(status.installation, InstallationStatus::NotInstalled);
        assert_eq!(status.refresh.error, Some(ProviderError::NotFound));

        let service = ProviderService::new(FakeRuntime::new(Ok("9.9.9")));
        let status = service
            .catalog(&selected_context(RuntimeMode::Live))
            .unwrap()
            .remove(0);
        assert_eq!(status.availability, ProviderAvailability::InvalidVersion);
        assert_eq!(status.descriptor.version, None);
    }

    #[test]
    fn snapshot_mode_never_probes_and_requires_configuration() {
        let service = ProviderService::new(FakeRuntime::new(Ok(TESTED_CODEBURN_VERSION)));
        let mut context = selected_context(RuntimeMode::Snapshot);
        let status = service.catalog(&context).unwrap().remove(0);
        assert_eq!(
            status.availability,
            ProviderAvailability::ConfigurationError
        );
        assert_eq!(service.runtime.detection_calls.get(), 0);

        context.snapshot_configured = true;
        let status = service.catalog(&context).unwrap().remove(0);
        assert_eq!(status.availability, ProviderAvailability::Available);
        assert_eq!(service.runtime.detection_calls.get(), 0);
    }

    #[test]
    fn installation_reconstructs_fixed_arguments_after_consent_and_policy_checks() {
        let service = ProviderService::new(FakeRuntime::new(Ok(TESTED_CODEBURN_VERSION)));
        let plan = service.installation_plan(CODEBURN_ID, None).unwrap();
        assert_eq!(
            plan.command_preview,
            ["npm", "install", "--global", "codeburn@0.9.24"]
        );
        let mut request = InstallationRequest {
            provider_id: CODEBURN_ID.to_owned(),
            requested_version: TESTED_CODEBURN_VERSION.to_owned(),
            confirmed: false,
            workspace_trusted: true,
            virtual_workspace: false,
        };
        assert_eq!(
            service.install(&request),
            Err(ProviderError::ConsentRequired)
        );
        request.confirmed = true;
        request.workspace_trusted = false;
        assert_eq!(
            service.install(&request),
            Err(ProviderError::WorkspaceBlocked)
        );
        request.workspace_trusted = true;
        request.virtual_workspace = true;
        assert_eq!(
            service.install(&request),
            Err(ProviderError::WorkspaceBlocked)
        );
        request.virtual_workspace = false;
        service.install(&request).unwrap();
        assert_eq!(service.runtime.installs.borrow().as_slice(), ["0.9.24"]);
    }

    #[test]
    fn unknown_provider_and_version_are_rejected_without_execution() {
        let service = ProviderService::new(FakeRuntime::new(Ok(TESTED_CODEBURN_VERSION)));
        let mut context = ProviderContext::default();
        context.selected.insert("remote-provider".to_owned());
        assert!(matches!(
            service.catalog(&context),
            Err(ProviderError::InvalidProvider)
        ));
        assert_eq!(
            service.installation_plan(CODEBURN_ID, Some("latest")),
            Err(ProviderError::InvalidVersion)
        );
        assert_eq!(service.runtime.detection_calls.get(), 0);
        assert!(service.runtime.installs.borrow().is_empty());
    }

    #[test]
    fn version_parser_accepts_only_plain_three_part_numeric_versions() {
        assert_eq!(
            parse_codeburn_version(b"CodeBurn v0.9.24\n").as_deref(),
            Some("0.9.24")
        );
        assert_eq!(parse_codeburn_version(b"CodeBurn latest"), None);
        assert_eq!(parse_codeburn_version(&[0xff]), None);
    }
}
