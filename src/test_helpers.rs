use difference::Changeset;
use regex::Regex;
use swc_common::comments::SingleThreadedComments;
use swc_common::{self, sync::Lrc, FileName, SourceMap};
use swc_ecma_codegen::Emitter;
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

use crate::Preprocessor;

pub fn testcase(input: &str, expected: &str) -> Result<(), swc_ecma_parser::error::Error> {
    let re = Regex::new(r"template_[0-9a-f]{32}").unwrap();
    let p = Preprocessor::new();
    let actual = p.process(input, Default::default())?;
    let actual_santized = re.replace_all(&actual.code, "template_UUID");
    let normalized_expected = normalize(expected);
    if actual_santized != normalized_expected {
        panic!(
            "code differs from expected:\n{}",
            format!(
                "{}",
                Changeset::new(&actual_santized, &normalized_expected, "\n")
            )
        );
    }

    assert!(!actual.map.is_empty(), "expected .map to not be empty");

    Ok(())
}

fn normalize(src: &str) -> String {
    let filename = "test.js".into();

    let source_map: Lrc<SourceMap> = Default::default();
    let comments: SingleThreadedComments = Default::default();

    let source_file = source_map.new_source_file(FileName::Real(filename).into(), src.to_string());

    let lexer = Lexer::new(
        Syntax::Typescript(TsSyntax {
            decorators: true,
            ..Default::default()
        }),
        Default::default(),
        StringInput::from(&*source_file),
        Some(&comments),
    );
    let mut parser = Parser::new_from(lexer);
    let parsed_module = parser.parse_module().unwrap();
    let mut buf = vec![];
    let mut emitter = Emitter {
        cfg: Default::default(),
        cm: source_map.clone(),
        wr: Box::new(swc_ecma_codegen::text_writer::JsWriter::new(
            source_map.clone(),
            "\n",
            &mut buf,
            None,
        )),
        comments: Some(&comments),
    };
    emitter.emit_module(&parsed_module).unwrap();
    let s = String::from_utf8_lossy(&buf);
    s.to_string()
}
