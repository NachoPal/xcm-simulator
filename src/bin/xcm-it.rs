use std::{fs, path::{Path, PathBuf}, collections::HashMap};
use regex::Regex;
use toml_edit::{Document, Value, Value::{InlineTable}, Item, Table, };
use pathdiff::diff_paths;
use std::process::Command;

const CONFIG_FILE: &'static str = "config.toml";
const CARGO_ENV_FILE: &'static str = "Cargo.env.toml";
const CARGO_FILE: &'static str = "Cargo.toml";
const ENV_KEY: &'static str = "env";

fn main() {
	let env_file = fs::read_to_string(CONFIG_FILE).expect(&format!("Failed to read file: {}", CONFIG_FILE));
    let mut env_doc = env_file.parse::<Document>().expect(&format!("Failed to parse file: {}", CONFIG_FILE));

    // Create a HashMap to store the variable values
    let mut env_variables: HashMap<String, Value> = HashMap::new();

    let table = env_doc.as_table_mut();
    collect_env_variables(table, &mut env_variables);

    // Lookup for Cargo.env.toml to generate Cargo.toml from them
    lookup_package("./", vec![CARGO_ENV_FILE], None, &mut env_variables);

	// ////
	let package_dir = "./integration-tests/assets";
    std::env::set_current_dir(package_dir).unwrap();

    let output = Command::new("cargo")
		.args(&["test"])
        .spawn()
        .expect("failed to execute cargo test");

	// panic!();
}

fn generate_cargo_files(mut path: PathBuf, variables: &mut HashMap<String, Value>) {
	// Read contents of Cargo.toml file into a string
	let contents = fs::read_to_string(path.clone()).unwrap();

	// Parse the contents into a TOML document
	let mut doc = contents.parse::<Document>().unwrap();

	println!("Where the file is {:?}", path);

	let mut doc_path = path.clone();
	doc_path.pop();


	// Recursively search for variables and their values
	replace_env_variables(doc_path, &mut doc.as_table_mut(), variables);

	path.set_file_name(CARGO_FILE);

	println!("File to write {:?}", path);

	fs::write(path, doc.to_string()).unwrap();
}

fn collect_env_variables(doc: &mut Table, variables: &mut HashMap<String, Value>) {
	for (key, item) in doc.iter_mut() {
		match item {
		// If the value is a table, recursively search for variables in its children
		Item::Table(table) => collect_env_variables(table, variables),
		// If the value is a table, recursively search for variables in its children
		Item::Value(InlineTable(inline_table)) => {
			let env_key = key.get().to_owned();
			variables.insert(env_key, Value::InlineTable(inline_table.clone()));
		},
		_ => {}
		}
	}
}

fn replace_env_variables(doc_path: PathBuf, doc: &mut Table, variables: &mut HashMap<String, Value>) {
    for (key, item) in doc.iter_mut() {
        match item {
            // If the value is a table, recursively search for variables in its children
            Item::Table(table) => replace_env_variables(doc_path.clone(), table, variables),
            // If the value is a table, recursively search for variables in its children
            Item::Value(InlineTable(inline_table)) => {
				if let Some((_, env_variable)) = inline_table.remove_entry(ENV_KEY) {
					let mut match_variable = false;
					for (k, v) in &mut *variables {
						if match_env_variable(k, env_variable.clone()) {
							match v {
								Value::InlineTable(env_inline_table) => {
								if let Some(root_path_value) = env_inline_table.get_mut("path") {
									let root_path = root_path_value.as_str().unwrap();
									// Lookup for generated Cargo.toml to replace the root path with the package path
									let package_path = lookup_package(
									root_path,
									vec![CARGO_FILE, CARGO_ENV_FILE],
									Some(key.get()),
									&mut HashMap::new()
									).expect(&format!("Failed to find package '{}' under '{}' directory", key, root_path));
									println!("The path of the package {:?}", package_path);
									let package_relative_path = diff_paths(
										package_path.clone(),
										doc_path.clone())
										.unwrap()
										.to_string_lossy()
										.into_owned();
									println!("Relative path {:?}", package_relative_path);
									*root_path_value = Value::from(package_relative_path);
								}
								inline_table.extend(env_inline_table.iter())
								},
								_ => {}
							};
							match_variable = true
						}
					}
					if !match_variable { panic!("No {} env variable was found in '{}' file", env_variable, CONFIG_FILE) }
				}
            },
            _ => {}
        }
    }
}

fn match_env_variable(key: &str, variable: Value) -> bool {
	let variable_str = &variable.as_str().unwrap();
	// Define a regular expression to match ${variable_name} format
	let re = Regex::new(r#"\$\{([\w-]+)\}"#).unwrap();

	if let Some(var) = re.captures(variable_str) {
		let a = var.get(1).map(|m| m.as_str()).unwrap();
		return a == key;
	}

	false
}

fn lookup_package_rec(dir: &Path, file_name: Vec<&str>, name: Option<&str>, variables: &mut HashMap<String, Value>) -> Option<PathBuf> {
	if let Ok(entries) = fs::read_dir(dir) {
		for entry in entries {
			if let Ok(entry) = entry {
				let path = entry.path();
				if path.is_dir() {
					if let Some(cargo_toml) = lookup_package_rec(&path, file_name.clone(), name, variables) {
						return Some(cargo_toml);
					}
				} else if let Some(file) = path.file_name() {
					if file_name.contains(&file.to_str().unwrap()) {
						if let Ok(contents) = fs::read_to_string(&path) {
							if name.is_some() && contents.contains(&format!("name = \"{}\"", name.unwrap())) {
								return Some(path);
							} else if name.is_none() {
								generate_cargo_files(path, variables);
							}
						}
					}
				}
			}
		}
	}
	None
}

fn lookup_package(root: &str, file_name: Vec<&str>, name: Option<&str>, variables: &mut HashMap<String, Value>) -> Option<String> {
	let dir = Path::new(root);
	if let Some(cargo_toml) = lookup_package_rec(dir, file_name, name, variables) {
		if let Some(parent_dir) = cargo_toml.parent() {
			return Some(parent_dir.to_string_lossy().into_owned());
		}
	}
	None
}
