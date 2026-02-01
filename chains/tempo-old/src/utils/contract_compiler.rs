use anyhow::{anyhow, Result};
use ethers_solc::artifacts::BytecodeObject;
use ethers_solc::{Project, ProjectPathsConfig, Solc};
use std::path::Path;
use tracing::info;

pub struct ContractCompiler;

impl ContractCompiler {
    pub fn compile(contract_path: &Path) -> Result<(String, String)> {
        info!("Compiling contract: {:?}", contract_path);

        if !contract_path.exists() {
            return Err(anyhow!("Contract file not found: {:?}", contract_path));
        }

        // 1. Determine which compiler to use
        let system_solc = std::process::Command::new("solc")
            .arg("--version")
            .output()
            .is_ok();
        let local_solc_path = Path::new("../../solc.exe");
        let has_local_solc = local_solc_path.exists();

        let solc_instance = if system_solc {
            info!("System 'solc' found. Using PATH.");
            Some(Solc::default())
        } else if has_local_solc {
            info!(
                "Local 'solc.exe' found at {:?}. Using local binary.",
                local_solc_path
            );
            // We need to resolve the absolute path for robustness or relative might work
            // canonicalize might fail if file doesn't exist, but we checked exists()
            let abs_path = local_solc_path
                .canonicalize()
                .unwrap_or(local_solc_path.to_path_buf());
            Some(Solc::new(abs_path))
        } else {
            None
        };

        if let Some(solc) = solc_instance {
            // Configure project to look for contracts in chains/tempo/contracts relative to CWD
            let root = Path::new(".");
            let contracts_dir = root.join("chains/tempo/contracts");

            let paths = ProjectPathsConfig::builder()
                .root(root)
                .sources(&contracts_dir)
                .artifacts(root.join("chains/tempo/out"))
                .build()?;

            let project = Project::builder()
                .solc(solc)
                .paths(paths)
                .set_auto_detect(true)
                .build()?;

            let output = project.compile_files(vec![contract_path.to_path_buf()])?;

            if output.has_compiler_errors() {
                let errors: Vec<String> = output
                    .output()
                    .errors
                    .iter()
                    .map(|e| e.message.clone())
                    .collect();
                return Err(anyhow!("Compilation failed: {:?}", errors));
            }

            let contract_name = contract_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow!("Invalid contract file name"))?;

            let artifact = output
                .find_first(contract_name)
                .ok_or_else(|| anyhow!("Contract artifact not found for {}", contract_name))?;

            let abi = serde_json::to_string(artifact.abi.as_ref().ok_or(anyhow!("No ABI"))?)?;

            let bin = match &artifact
                .bytecode
                .as_ref()
                .ok_or(anyhow!("No bytecode"))?
                .object
            {
                BytecodeObject::Bytecode(bytes) => bytes.to_string(),
                BytecodeObject::Unlinked(s) => s.clone(),
            };

            Ok((abi, bin))
        } else {
            tracing::warn!("'solc' binary not found. Falling back to Node.js compiler (solc-js).");

            // Resolve paths - use chains/tempo/contracts for the compile.js script
            let script_path = Path::new("chains/tempo/contracts/compile.js");
            if !script_path.exists() {
                return Err(anyhow!(
                    "Fallback compiler script not found at {:?}",
                    script_path
                ));
            }

            let output = std::process::Command::new("node")
                .arg(script_path)
                .arg(contract_path)
                .output();

            let output = match output {
                Ok(o) => o,
                Err(e) => {
                    return Err(anyhow!(
                        "Failed to execute Node.js: {}. Validate Node.js is installed.",
                        e
                    ))
                }
            };

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("Node.js compilation failed: {}", stderr));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse: { abi: String(JSON), bin: String(Hex) }
            let result: serde_json::Value = serde_json::from_str(&stdout)?;

            let abi = result["abi"]
                .as_str()
                .ok_or(anyhow!("Invalid ABI in JS output"))?
                .to_string();
            let bytecode = result["bin"]
                .as_str()
                .ok_or(anyhow!("Invalid bytecode in JS output"))?
                .to_string();

            Ok((abi, bytecode))
        }
    }
}
