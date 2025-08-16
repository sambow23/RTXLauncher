pub mod settings;
pub mod jobs;
pub mod elevation;
pub mod steam;
pub mod fs_linker;
pub mod install;
pub mod mount;
pub mod github;
pub mod remix_installer;
pub mod rtxio;
pub mod usda;
pub mod update;
pub mod launch;
pub mod logging;
pub mod patching;

pub use settings::{AppSettings, SettingsStore};
pub use jobs::{JobHandle, JobProgress, JobRunner};
pub use elevation::{is_elevated, relaunch_as_admin};
pub use steam::{detect_gmod_install_folder, detect_install_folder_path};
pub use fs_linker::{link_dir_best_effort, link_file_best_effort, copy_dir_with_progress};
pub use install::{InstallPlan, perform_basic_install};
pub use mount::{mount_game, unmount_game, is_game_mounted};
pub use github::{fetch_releases, GitHubAsset, GitHubRelease, GitHubRateLimit, set_personal_access_token, load_personal_access_token};
pub use remix_installer::{select_best_asset, analyze_zip_for_layout, install_remix_from_release, install_fixes_from_release, select_best_package_asset};
pub use rtxio::{has_rtxio_packages, extract_packages};
pub use usda::apply_usda_fixes;
pub use update::{detect_updates, apply_updates, FileUpdateInfo};
pub use launch::{build_launch_args, launch_game};
#[cfg(unix)]
pub use launch::list_proton_builds;
pub use logging::init_logging;
pub use patching::{apply_patches_from_repo, PatchResult};


