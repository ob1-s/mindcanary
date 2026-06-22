use std::{env, fs, path::PathBuf};

use mindcanary_protocol::typescript_schema;

fn main() {
    let output_path = env::args_os().nth(1).map_or_else(
        || PathBuf::from("packages/protocol-ts/src/generated.ts"),
        PathBuf::from,
    );

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).expect("create TypeScript output directory");
    }

    fs::write(output_path, typescript_schema()).expect("write generated TypeScript protocol");
}
