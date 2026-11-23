use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const JAVA_SOURCE_ROOT: &str = "src/main/java/games/jvrcruz/siliconcraft";

pub(super) fn compile_project(
    project: &HashMap<String, Vec<String>>,
    entry: &str,
) -> Result<Vec<String>, String> {
    let source = source_root()?;
    let tmp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let driver = tmp.path().join("CompileN2.java");
    let stub_dir = tmp.path().join("stubs");
    fs::create_dir_all(&stub_dir).map_err(|error| error.to_string())?;
    // The upstream NetworkPort drags in the server's HTTP/3 stack. The compiler only needs its constants.
    let network_port = stub_dir.join("NetworkPort.java");
    fs::write(&network_port, include_str!("NetworkPort.java")).map_err(|error| error.to_string())?;
    install_resources(&source, tmp.path())?;
    let input = tmp.path().join("project.bin");
    fs::write(&driver, include_str!("CompileN2.java")).map_err(|error| error.to_string())?;
    write_project(&input, project).map_err(|error| error.to_string())?;

    let n2 = source.join("compiler/n2");
    let mut sources = vec![
        driver,
        source.join("compiler/InstructionValidator.java"),
        source.join("vm/PeripheralBus.java"),
        network_port,
    ];
    for item in fs::read_dir(n2).map_err(|error| error.to_string())? {
        let path = item.map_err(|error| error.to_string())?.path();
        if path.extension().is_some_and(|ext| ext == "java") {
            sources.push(path);
        }
    }
    let compiled = Command::new(java_tool("javac"))
        .args(["--release", "21", "-encoding", "UTF-8", "-proc:none", "-d"])
        .arg(tmp.path())
        .args(sources)
        .output()
        .map_err(|error| format!("N2 compilation requires a Java 21 JDK: {error}"))?;
    if !compiled.status.success() {
        return Err(String::from_utf8_lossy(&compiled.stderr).into_owned());
    }

    let assembly = Command::new(java_tool("java"))
        .arg("-cp")
        .arg(tmp.path())
        .arg("CompileN2")
        .arg(input)
        .arg(entry)
        .output()
        .map_err(|error| error.to_string())?;
    if !assembly.status.success() {
        return Err(String::from_utf8_lossy(&assembly.stderr).into_owned());
    }
    let output = String::from_utf8(assembly.stdout).map_err(|error| error.to_string())?;
    Ok(output.lines().map(str::to_owned).collect())
}

/// The N2 compiler links `network.n2` from the classpath, so ship the checkout's resources with it.
fn install_resources(source: &Path, target: &Path) -> Result<(), String> {
    let resources = source.join("../../../../resources/siliconcraft/n2");
    let destination = target.join("siliconcraft/n2");
    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    for item in fs::read_dir(&resources)
        .map_err(|error| format!("N2 library resources not found at {}: {error}", resources.display()))?
    {
        let path = item.map_err(|error| error.to_string())?.path();
        if path.extension().is_some_and(|ext| ext == "n2") {
            let name = path.file_name().expect("resource file name");
            fs::copy(&path, destination.join(name)).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn source_root() -> Result<PathBuf, String> {
    let configured = env::var_os("SILICONCRAFT_SOURCE").map(PathBuf::from);
    let checkout = if let Some(path) = configured {
        path
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../SiliconCraft")
    };
    let root = checkout.join(JAVA_SOURCE_ROOT);
    if root.join("compiler/n2/N2Compiler.java").is_file() {
        Ok(root)
    } else {
        Err(format!(
            "SiliconCraft's N2 compiler was not found at {}. Set SILICONCRAFT_SOURCE to the SiliconCraft checkout.",
            checkout.display()
        ))
    }
}

fn java_tool(name: &str) -> PathBuf {
    let executable = format!("{name}{}", env::consts::EXE_SUFFIX);
    env::var_os("JAVA_HOME").map_or_else(
        || PathBuf::from(&executable),
        |home| PathBuf::from(home).join("bin").join(&executable),
    )
}

fn write_project(path: &Path, project: &HashMap<String, Vec<String>>) -> io::Result<()> {
    let mut file = File::create(path)?;
    write_length(&mut file, project.len())?;
    for (name, lines) in project {
        for text in [name.as_str(), &lines.join("\n")] {
            write_length(&mut file, text.len())?;
            file.write_all(text.as_bytes())?;
        }
    }
    Ok(())
}

fn write_length(file: &mut File, length: usize) -> io::Result<()> {
    let length = i32::try_from(length)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "N2 compiler input too large"))?;
    file.write_all(&length.to_be_bytes())
}
