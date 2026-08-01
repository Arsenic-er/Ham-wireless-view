// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use std::process::ExitCode;

use hamheatmap_validation_server::{ServerConfig, ValidationServer, help_text};

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        print!("{}", help_text());
        return ExitCode::SUCCESS;
    }
    let config = match ServerConfig::from_args(arguments) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("configuration error: {error}\n\n{}", help_text());
            return ExitCode::from(2);
        }
    };
    let (server, listener) = match ValidationServer::bind(&config) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("startup error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => {
            eprintln!("startup error: cannot inspect listener: {error}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!(
        "HamHeatmap validation server listening on http://{address} (dist={}, data={})",
        config.dist_dir.display(),
        config.data_root.display()
    );
    match server.serve(listener) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("server error: {error}");
            ExitCode::FAILURE
        }
    }
}
