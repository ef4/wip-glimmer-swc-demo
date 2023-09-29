use content_tag::{Options, Preprocessor};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;

use swc_common::errors::{ColorConfig, Handler};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Must pass input filename");
        exit(-1);
    }
    let filename: PathBuf = args[1].clone().into();

    let src = fs::read_to_string(filename.clone()).unwrap();

    let p = Preprocessor::new(Arc::new(Options {
        inline_source_map: true,
    }));

    let result = p.process(&src, Some(filename));

    match result {
        Ok(output) => println!("{}", output),
        Err(err) => {
            let handler =
                Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(p.source_map()));
            err.into_diagnostic(&handler).emit();
        }
    }
}
