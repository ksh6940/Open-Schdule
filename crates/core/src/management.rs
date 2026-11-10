use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub fn is_management_command(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("quit") => true,
        Some("delete") => args.len() == 1 || args.get(1).is_some_and(|s| s.starts_with("--")),
        _ => false,
    }
}

fn help() {
    println!("openlab quit [--force]");
    println!("openlab delete --cache [--yes]");
    println!("openlab delete --data [--yes]");
    println!("openlab delete --version <버전 또는 배포 폴더 경로> [--yes]");
    println!("openlab delete --program <배포 폴더 경로> [--yes]");
    println!("일정 삭제: openlab delete <ID>");
}

pub fn run(args: &[String]) -> Result<()> {
    if args[0] == "quit" {
        if args.len() == 2 && args[1] == "--help" {
            help();
            return Ok(());
        }
        let force = match &args[1..] {
            [] => false,
            [flag] if flag == "--force" => true,
            _ => bail!("사용법: openlab quit [--force]"),
        };
        return quit(force);
    }
    if args.len() == 1 || (args.len() == 2 && args[1] == "--help") {
        help();
        return Ok(());
    }
    let (kind, target, yes) = parse_delete(&args[1..])?;
    delete(kind, target.as_deref(), yes)
}

#[derive(Debug, PartialEq, Eq)]
enum DeleteKind {
    Cache,
    Data,
    Version,
    Program,
}

fn parse_delete(args: &[String]) -> Result<(DeleteKind, Option<PathBuf>, bool)> {
    let yes = args.last().is_some_and(|arg| arg == "--yes");
    let args = if yes { &args[..args.len() - 1] } else { args };
    match args {
        [option] if option == "--cache" => Ok((DeleteKind::Cache, None, yes)),
        [option] if option == "--data" => Ok((DeleteKind::Data, None, yes)),
        [option, path] if option == "--version" && !path.starts_with("--") => {
            Ok((DeleteKind::Version, Some(PathBuf::from(path)), yes))
        }
        [option, path] if option == "--program" && !path.starts_with("--") => {
            Ok((DeleteKind::Program, Some(PathBuf::from(path)), yes))
        }
        _ => bail!("삭제 옵션이 올바르지 않습니다. openlab delete --help 를 확인하세요."),
    }
}

fn confirm(paths: &[PathBuf], yes: bool) -> Result<bool> {
    if paths.is_empty() {
        println!("삭제할 대상이 없습니다.");
        return Ok(false);
    }
    println!("다음 항목을 영구 삭제합니다:");
    for path in paths {
        println!("  {}", path.display());
    }
    if yes {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        bail!("대화형 터미널에서 확인하거나 --yes를 지정하세요.");
    }
    print!("계속하려면 DELETE를 입력하세요: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim() != "DELETE" {
        println!("취소했습니다.");
        return Ok(false);
    }
    Ok(true)
}

fn reject_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => bail!(
                "링크가 포함된 경로는 삭제할 수 없습니다: {}",
                ancestor.display()
            ),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("경로 검사 실패: {}", ancestor.display()))
            }
        }
    }
    Ok(())
}

fn validate_package(path: &Path) -> Result<PathBuf> {
    reject_links(path)?;
    let path = path
        .canonicalize()
        .context("배포 폴더를 찾을 수 없습니다")?;
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if !name.starts_with("OpenSchedule-v") || !name.ends_with("-win32-x64") {
        bail!("OpenSchedule 버전 배포 폴더만 삭제할 수 있습니다.");
    }
    let manifest = path.join("resources").join("app").join("package.json");
    reject_links(&manifest)?;
    let value: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest).context("배포 매니페스트를 읽을 수 없습니다")?,
    )?;
    if value["name"] != "openschedule" || !path.join("OpenSchedule-App.exe").is_file() {
        bail!("유효한 OpenSchedule 배포 폴더가 아닙니다.");
    }
    if std::env::current_exe()?.canonicalize()?.starts_with(&path) {
        bail!("실행 중인 CLI가 포함된 폴더는 삭제할 수 없습니다. 다른 버전 또는 프로젝트의 openlab으로 실행하세요.");
    }
    Ok(path)
}

fn version_path(value: &Path) -> Result<PathBuf> {
    let version = value.to_str().context("버전이 올바르지 않습니다")?;
    if version.chars().all(|c| c.is_ascii_digit() || c == '.')
        && version.chars().any(|c| c.is_ascii_digit())
    {
        let exe = std::env::current_exe()?;
        let dir = exe.parent().context("실행 파일 폴더를 찾을 수 없습니다")?;
        let dist = if dir.join("OpenSchedule-App.exe").is_file() {
            dir.parent()
                .context("배포 폴더의 상위 경로를 찾을 수 없습니다")?
                .to_path_buf()
        } else {
            crate::paths::workspace_root()
                .context("프로젝트 폴더를 찾을 수 없습니다")?
                .join("dist")
        };
        Ok(dist.join(format!("OpenSchedule-v{version}-win32-x64")))
    } else {
        Ok(value.to_path_buf())
    }
}

fn collect_data_files(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    reject_links(dir)?;
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("일정 폴더 읽기 실패: {}", dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().ends_with("_schedule.json") {
            let path = entry.path();
            reject_links(&path)?;
            if !entry.file_type()?.is_file() {
                bail!("일정 파일이 일반 파일이 아닙니다: {}", path.display());
            }
            paths.push(path);
        }
    }
    Ok(())
}

fn delete(kind: DeleteKind, target: Option<&Path>, yes: bool) -> Result<()> {
    ensure_no_running_programs()?;
    let base = directories::BaseDirs::new().context("사용자 데이터 폴더를 찾을 수 없습니다")?;
    let data = base.config_dir().join("OpenSchedule");
    let mut targets = Vec::new();
    match kind {
        DeleteKind::Version => targets.push(validate_package(&version_path(
            target.context("버전이 필요합니다")?,
        )?)?),
        DeleteKind::Program => targets.push(validate_package(
            target.context("배포 폴더 경로가 필요합니다")?,
        )?),
        DeleteKind::Cache => {
            let electron_data = base.config_dir().join("OpenSchedule");
            for name in [
                "Cache",
                "Code Cache",
                "GPUCache",
                "GPUPersistentCache",
                "DawnCache",
                "DawnWebGPUCache",
                "DawnGraphiteCache",
                "GrShaderCache",
                "ShaderCache",
            ] {
                let path = electron_data.join(name);
                reject_links(&path)?;
                if path.exists() {
                    if !path.is_dir() {
                        bail!("캐시 경로가 폴더가 아닙니다: {}", path.display());
                    }
                    targets.push(path);
                }
            }
        }
        DeleteKind::Data => {
            for name in [
                "todos.db",
                "todos.db-wal",
                "todos.db-shm",
                "todos.db-journal",
                "settings.json",
                "agent.json",
                "agent_info.json",
            ] {
                let path = data.join(name);
                reject_links(&path)?;
                if path.exists() {
                    if !path.is_file() {
                        bail!("데이터 경로가 파일이 아닙니다: {}", path.display());
                    }
                    targets.push(path);
                }
            }
            collect_data_files(&data.join("schedules"), &mut targets)?;
            if let Some(root) = crate::paths::workspace_root() {
                collect_data_files(&root.join("schedules"), &mut targets)?;
            }
        }
    }
    if !confirm(&targets, yes)? {
        return Ok(());
    }
    ensure_no_running_programs()?;
    remove_targets(targets)
}

fn remove_targets(targets: Vec<PathBuf>) -> Result<()> {
    for path in targets {
        reject_links(&path)?;
        if path.is_dir() {
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("삭제 실패: {}", path.display()))?;
        } else {
            std::fs::remove_file(&path)
                .with_context(|| format!("삭제 실패: {}", path.display()))?;
        }
        println!("삭제됨: {}", path.display());
    }
    Ok(())
}

#[cfg(not(windows))]
fn quit(_force: bool) -> Result<()> {
    bail!("프로그램 종료 명령은 현재 Windows에서만 지원됩니다.")
}

#[cfg(not(windows))]
fn ensure_no_running_programs() -> Result<()> {
    bail!("안전한 삭제는 현재 Windows에서만 지원됩니다.")
}

#[cfg(windows)]
mod windows {
    use super::*;
    use serde::Deserialize;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Process {
        process_id: u32,
        name: String,
        executable_path: Option<String>,
        command_line: Option<String>,
    }

    fn processes() -> Result<Vec<Process>> {
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "$ErrorActionPreference='Stop'; [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false); ConvertTo-Json -InputObject @(Get-CimInstance Win32_Process | Where-Object { $_.Name -in @('OpenSchedule-App.exe','electron.exe','openschedule-agent.exe','openlab.exe') } | Select-Object ProcessId,Name,ExecutablePath,CommandLine) -Compress"])
            .output().context("프로세스 목록을 확인할 수 없습니다")?;
        if !output.status.success() {
            bail!(
                "프로세스 조회 실패: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        serde_json::from_slice(&output.stdout).context("프로세스 조회 결과를 읽을 수 없습니다")
    }

    fn is_app(process: &Process) -> bool {
        let name = &process.name;
        let Some(command_line) = process.command_line.as_deref() else {
            return true;
        };
        let cmd = command_line.to_ascii_lowercase().replace('/', "\\");
        if name.eq_ignore_ascii_case("OpenSchedule-App.exe") {
            return !cmd.contains("--type=");
        }
        if name.eq_ignore_ascii_case("electron.exe") {
            return !cmd.contains("--type=") && cmd.contains("electron\\main.js");
        }
        (name.eq_ignore_ascii_case("openschedule-agent.exe")
            || name.eq_ignore_ascii_case("openlab.exe"))
            && (cmd.contains("--headless") || cmd.contains("--api-only"))
    }

    pub(super) fn ensure_no_running_programs() -> Result<()> {
        let running: Vec<_> = processes()?.into_iter().filter(is_app).collect();
        if !running.is_empty() {
            let ids: Vec<_> = running.iter().map(|p| p.process_id.to_string()).collect();
            bail!(
                "실행 중인 OpenSchedule 프로세스가 있습니다 (PID {}). 먼저 모두 종료하세요.",
                ids.join(", ")
            );
        }
        Ok(())
    }

    fn same_path(a: &str, b: &Path) -> bool {
        let left = Path::new(a)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(a));
        left.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }

    pub(super) fn quit(force: bool) -> Result<()> {
        let exe = std::env::current_exe()?;
        let dir = exe.parent().context("실행 파일 폴더를 찾을 수 없습니다")?;
        let packaged = dir.join("OpenSchedule-App.exe");
        let root = crate::paths::workspace_root();
        let is_packaged = packaged.is_file();
        let gui = if is_packaged {
            packaged
        } else {
            root.as_ref()
                .context("개발 폴더를 찾을 수 없습니다")?
                .join("node_modules")
                .join("electron")
                .join("dist")
                .join("electron.exe")
        };
        if !gui.is_file() {
            bail!("GUI 실행 파일을 찾을 수 없습니다: {}", gui.display());
        }
        let gui = gui.canonicalize()?;
        let main = root.as_ref().map(|r| r.join("electron").join("main.js"));
        let matches_gui = |p: &Process| {
            p.executable_path
                .as_deref()
                .is_some_and(|path| same_path(path, &gui))
                && is_app(p)
        };
        let current = processes()?;
        if current
            .iter()
            .any(|p| is_app(p) && p.executable_path.is_none())
        {
            bail!("일부 프로그램의 실행 경로를 확인할 수 없어 종료를 중단했습니다.");
        }
        let before: Vec<_> = current.into_iter().filter(&matches_gui).collect();
        let mut engine_paths = vec![dir.join("openschedule-agent.exe"), dir.join("openlab.exe")];
        if !is_packaged {
            if let Some(root) = &root {
                for profile in ["target\\release", "target\\debug", "target_mgr\\release"] {
                    for name in ["openschedule-agent.exe", "openlab.exe"] {
                        engine_paths.push(root.join(profile).join(name));
                    }
                }
            }
        }
        let engine_paths: Vec<_> = engine_paths
            .into_iter()
            .filter_map(|path| path.canonicalize().ok())
            .collect();
        let matches_engine = |p: &Process| {
            (p.name.eq_ignore_ascii_case("openschedule-agent.exe")
                || p.name.eq_ignore_ascii_case("openlab.exe"))
                && is_app(p)
                && p.executable_path.as_deref().is_some_and(|path| {
                    engine_paths
                        .iter()
                        .any(|candidate| same_path(path, candidate))
                })
        };
        if before.is_empty() && !force {
            println!("이 설치에서 실행 중인 GUI가 없습니다. 별도로 실행한 엔진은 유지됩니다.");
            return Ok(());
        }
        if force {
            let mut candidates = before;
            candidates.extend(processes()?.into_iter().filter(&matches_engine));
            for process in &candidates {
                // Recheck identity immediately before targeting a PID, never by image name.
                let current = processes()?;
                let Some(current_process) =
                    current.iter().find(|p| p.process_id == process.process_id)
                else {
                    continue;
                };
                if !(matches_gui(current_process) || matches_engine(current_process)) {
                    bail!(
                        "프로세스가 변경되어 강제 종료를 중단했습니다: {}",
                        process.process_id
                    );
                }
                let output = Command::new("taskkill")
                    .args(["/PID", &process.process_id.to_string(), "/T", "/F"])
                    .output()?;
                if !output.status.success() {
                    bail!(
                        "PID {} 종료 실패: {}",
                        process.process_id,
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            let remaining = processes()?;
            if remaining
                .iter()
                .any(|p| matches_gui(p) || matches_engine(p))
            {
                bail!("일부 프로그램이 아직 실행 중입니다. 작업 관리자에서 확인하세요.");
            }
            println!("이 설치에서 확인된 GUI와 엔진을 강제 종료했습니다.");
        } else {
            let mut command = Command::new(&gui);
            if gui
                .file_name()
                .is_some_and(|s| s.eq_ignore_ascii_case("electron.exe"))
            {
                command.arg(main.context("Electron 진입점을 찾을 수 없습니다")?);
            }
            command
                .arg("--openlab-quit")
                .env_remove("ELECTRON_RUN_AS_NODE")
                .spawn()?;
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                let current = processes()?;
                if !current.iter().any(|p| {
                    before.iter().any(|old| old.process_id == p.process_id) && matches_gui(p)
                }) {
                    println!(
                        "GUI와 직접 시작한 엔진을 종료했습니다. 별도로 실행한 엔진은 유지됩니다."
                    );
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    bail!("정상 종료되지 않았습니다. 이전 버전이면 openlab quit --force를 사용하세요.");
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn process(name: &str, cmd: Option<&str>) -> Process {
            Process {
                process_id: 1,
                name: name.into(),
                executable_path: Some(format!("C:\\test\\{name}")),
                command_line: cmd.map(str::to_string),
            }
        }

        #[test]
        fn classifies_only_main_and_headless_processes() {
            assert!(is_app(&process(
                "electron.exe",
                Some("electron.exe electron/main.js")
            )));
            assert!(!is_app(&process(
                "electron.exe",
                Some("electron.exe --type=renderer")
            )));
            assert!(!is_app(&process("openlab.exe", Some("openlab.exe list"))));
            assert!(is_app(&process(
                "openlab.exe",
                Some("openlab.exe --headless")
            )));
            assert!(is_app(&process("OpenSchedule-App.exe", None)));
        }

        #[test]
        fn process_query_is_valid_json() {
            processes().unwrap();
        }
    }
}

#[cfg(windows)]
use windows::{ensure_no_running_programs, quit};

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn management_does_not_intercept_todo_delete() {
        assert!(is_management_command(&args(&["delete", "--data"])));
        assert!(is_management_command(&args(&["delete"])));
        assert!(is_management_command(&args(&["quit", "--force"])));
        assert!(!is_management_command(&args(&["delete", "123"])));
    }

    #[test]
    fn delete_options_are_explicit() {
        assert_eq!(
            parse_delete(&args(&["--data", "--yes"])).unwrap(),
            (DeleteKind::Data, None, true)
        );
        assert_eq!(
            parse_delete(&args(&["--version", "1.02"])).unwrap(),
            (DeleteKind::Version, Some(PathBuf::from("1.02")), false)
        );
        assert_eq!(
            parse_delete(&args(&["--program", "some-path"])).unwrap(),
            (DeleteKind::Program, Some(PathBuf::from("some-path")), false)
        );
        assert!(parse_delete(&args(&["--data", "--cache"])).is_err());
        assert!(parse_delete(&args(&["--version"])).is_err());
        assert!(parse_delete(&args(&["--yes"])).is_err());
    }

    #[test]
    fn schedule_collection_is_limited_to_schedule_files() {
        let dir = std::env::temp_dir().join(format!(
            "openschedule-management-test-{}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("2026-01-01_schedule.json"), "{}").unwrap();
        std::fs::write(dir.join("other.json"), "{}").unwrap();
        let mut files = Vec::new();
        collect_data_files(&dir, &mut files).unwrap();
        assert_eq!(files, vec![dir.join("2026-01-01_schedule.json")]);
        std::fs::remove_file(dir.join("2026-01-01_schedule.json")).unwrap();
        std::fs::remove_file(dir.join("other.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn package_validation_and_deletion_use_only_explicit_target() {
        let parent =
            std::env::temp_dir().join(format!("openschedule-package-test-{}", std::process::id()));
        let package = parent.join("OpenSchedule-v9.99-win32-x64");
        let other = parent.join("keep.txt");
        std::fs::create_dir_all(package.join("resources").join("app")).unwrap();
        std::fs::write(package.join("OpenSchedule-App.exe"), "").unwrap();
        std::fs::write(
            package.join("resources").join("app").join("package.json"),
            r#"{"name":"openschedule"}"#,
        )
        .unwrap();
        std::fs::write(&other, "keep").unwrap();
        assert!(validate_package(&parent).is_err());
        let target = validate_package(&package).unwrap();
        remove_targets(vec![target]).unwrap();
        assert!(!package.exists());
        assert!(other.exists());
        std::fs::remove_file(other).unwrap();
        std::fs::remove_dir(parent).unwrap();
    }
}
