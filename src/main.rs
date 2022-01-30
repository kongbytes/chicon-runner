mod models;
mod workspace;

use std::process::Command;
use std::fs::{OpenOptions};
use std::io::prelude::*;
use std::time::SystemTime;
use std::convert::{TryInto};
use std::collections::HashMap;

use mongodb::{ bson::doc, sync::Client };
use tungstenite::{connect};
use url::Url;

use models::{CodeFunction, Repository, Scan};

fn main() {

    let client = Client::with_uri_str("mongodb://code:password@localhost:27017/code").unwrap();
    let database = client.database("code");

    let function_collection = database.collection::<CodeFunction>("functions");
    let repo_collection = database.collection::<Repository>("repositories");
    let scan_collection = database.collection::<Scan>("scans");

    let repositories: Vec<Repository> = repo_collection.find(doc! {}, None)
        .unwrap()
        .map(|result| result.unwrap())
        .collect();

    let code_functions: Vec<CodeFunction> = function_collection.find(doc! {}, None)
        .unwrap()
        .map(|result| result.unwrap())
        .collect();

    let (mut socket, _response) = connect(Url::parse("ws://localhost:3000/socket").unwrap()).expect("Can't connect");
    println!("Connected to the scheduler");

    // EXECUTION SIMULATION:

    loop {

        let repository_id: tungstenite::Message = socket.read_message().expect("Error reading message");
        println!("Received request for repository ID {}", repository_id);

        let nodejs_repos: Vec<&Repository> = repositories.iter().filter(|repo| repo.public_id == repository_id.clone().into_text().unwrap()).collect();

        // TODO Repo caching can be more optimal
        workspace::clean(true);

        for targeted_repo in nodejs_repos {

            println!("Repository {} with ID {} ({:?}, {:?})", targeted_repo.public_id, targeted_repo.name, targeted_repo.branch, targeted_repo.directory);
            targeted_repo.clone();

            for code_function in code_functions.iter() {

                println!(" - Executing function \"{}\" in {} environment (ID {})", code_function.name, code_function.environment.name, code_function.public_id);
                workspace::clean(false);

                // Write extractor logs
                let mut function_file = OpenOptions::new()
                        .read(false)
                        .write(true)
                        .create(true)
                        .open(format!("workspace/bin/process.{}", &code_function.environment.file_extension)).unwrap();
                function_file.write_all(code_function.content.as_bytes()).unwrap();

                let mut nerdctl = Command::new("nerdctl");
                nerdctl
                    .arg("--namespace=kb")
                    .arg("run")
                    .arg("--rm")
                    // Security flags
                    .arg("--cap-drop")
                    .arg("all")
                    .arg("--security-opt")
                    .arg("apparmor=docker-default");

                if code_function.capabilities.network {
                    nerdctl.arg("--network").arg("bridge");
                }
                else {
                    nerdctl.arg("--network").arg("none");
                }
                
                nerdctl.arg("--volume") // Volume mounting
                    .arg("/home/corentin/sandbox/code-scanner/workspace/repository:/workspace:ro")
                    .arg("--volume")
                    .arg("/home/corentin/sandbox/code-scanner/workspace/bin:/tmp-bin:ro")
                    .arg("--volume")
                    .arg("/home/corentin/sandbox/code-scanner/workspace/result:/result")
                    .arg("--workdir")
                    .arg("/workspace");

                if !code_function.capabilities.filesystem {
                    nerdctl.arg("--read-only");
                }
                
                // Binary
                nerdctl.arg(&code_function.environment.base_image)
                    .arg(&code_function.environment.executor)
                    .arg(format!("/tmp-bin/process.{}", &code_function.environment.file_extension));
                
                let start_time = SystemTime::now();
                let output = nerdctl
                    .output()
                    .expect("failed to execute process");
                let timing_ms: usize = SystemTime::now()
                    .duration_since(start_time)
                    .unwrap()
                    .as_millis()
                    .try_into()
                    .unwrap();

                let stderr_logs = String::from_utf8(output.stderr).unwrap();
                let stdout_logs = String::from_utf8(output.stdout).unwrap();
                let logs = format!("{}\n{}", stdout_logs, stderr_logs);

                let mut results: HashMap<String, String> = HashMap::new();
                let potential_toml = std::fs::read_to_string("workspace/result/data.toml");

                if let Ok(toml_content) = potential_toml {

                    let toml_lines = toml_content.lines();
                    for toml_line in toml_lines {

                        let mut splitted_line = toml_line.split("=");
                        let result_key = splitted_line.next().unwrap();
                        let result_value = splitted_line.next().unwrap();

                        results.insert(result_key.to_string(), result_value.to_string());
                    }
                }

                scan_collection.insert_one(Scan {
                    function_id: code_function.public_id.to_string(),
                    repository_id: targeted_repo.public_id.to_string(),
                    has_failed: !output.status.success() ,
                    logs,
                    timing_ms,
                    results
                }, None).unwrap();
            }

            workspace::clean(true);
        }
    }
}
