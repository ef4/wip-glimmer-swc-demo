#![feature(box_patterns)]

#[macro_use]
extern crate lazy_static;

use base64::{engine::general_purpose, Engine as _};
use std::path::PathBuf;
use swc_common::comments::SingleThreadedComments;
use swc_common::source_map::SourceMapGenConfig;
use swc_common::{self, sync::Lrc, FileName, Mark, SourceMap};
use swc_core::common::GLOBALS;
use swc_ecma_ast::{
    Ident, ImportDecl, ImportNamedSpecifier, ImportSpecifier, Module, ModuleDecl, ModuleExportName,
    ModuleItem,
};
use swc_ecma_codegen::Emitter;
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsConfig};
use swc_ecma_transforms::resolver;
use swc_ecma_utils::private_ident;
use swc_ecma_visit::{as_folder, VisitMutWith, VisitWith};
use uuid::Uuid;

mod bindings;
mod locate;
mod snippets;
mod transform;

#[derive(Default)]
pub struct Options {
    pub filename: Option<PathBuf>,
    pub inline_source_map: bool,
}

pub struct Preprocessor {
    source_map: Lrc<SourceMap>,
    comments: SingleThreadedComments,
}

pub struct CodeMapPair {
    pub code: String,
    pub map: String,
}

struct SourceMapConfig;
impl SourceMapGenConfig for SourceMapConfig {
    fn file_name_to_source(&self, f: &swc_common::FileName) -> String {
        f.to_string()
    }

    fn inline_sources_content(&self, _: &swc_common::FileName) -> bool {
        true
    }
}

impl Preprocessor {
    pub fn new() -> Self {
        Self {
            source_map: Default::default(),
            comments: SingleThreadedComments::default(),
        }
    }

    pub fn parse(
        &self,
        src: &str,
        options: Options,
    ) -> Result<Vec<locate::Occurrence>, swc_ecma_parser::error::Error> {
        let filename = match options.filename {
            Some(name) => FileName::Real(name),
            None => FileName::Anon,
        };

        let source_file = self.source_map.new_source_file(filename, src.to_string());

        let lexer = Lexer::new(
            Syntax::Typescript(TsConfig {
                decorators: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*source_file),
            Some(&self.comments),
        );
        let mut parser = Parser::new_from(lexer);
        GLOBALS.set(&Default::default(), || {
            let parsed_module = parser.parse_module()?;

            let mut visitor = locate::LocateContentTagVisitor::default();

            parsed_module.visit_with(&mut visitor);

            Ok(visitor.occurrences)
        })
    }

    pub fn process(
        &self,
        src: &str,
        options: Options,
    ) -> Result<CodeMapPair, swc_ecma_parser::error::Error> {
        let target_specifier = "template";
        let target_module = "@ember/template-compiler";
        let filename = match options.filename {
            Some(name) => FileName::Real(name),
            None => FileName::Anon,
        };

        let source_file = self.source_map.new_source_file(filename, src.to_string());

        let lexer = Lexer::new(
            Syntax::Typescript(TsConfig {
                decorators: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*source_file),
            Some(&self.comments),
        );
        let mut parser = Parser::new_from(lexer);
        GLOBALS.set(&Default::default(), || {
            let mut parsed_module = parser.parse_module()?;

            let id = private_ident!(format!(
                "{}_{}",
                target_specifier,
                Uuid::new_v4().to_string().replace("-", "")
            ));
            let mut needs_import = false;
            parsed_module.visit_mut_with(&mut as_folder(transform::TransformVisitor::new(
                &id,
                Some(&mut needs_import),
            )));

            if needs_import {
                insert_import(&mut parsed_module, target_module, target_specifier, &id)
            }

            let unresolved_mark = Mark::new();
            let top_level_mark = Mark::new();

            parsed_module.visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, false));

            let codemap = self.print(&parsed_module, options.inline_source_map);

            Ok(codemap)
        })
    }

    fn print(&self, module: &Module, inline_source_map: bool) -> CodeMapPair {
        let mut buf = vec![];
        let mut srcmap = vec![];
        let mut source_map_buffer = vec![];
        let mut emitter = Emitter {
            cfg: Default::default(),
            cm: self.source_map.clone(),
            wr: swc_ecma_codegen::text_writer::JsWriter::new(
                self.source_map().clone(),
                "\n",
                &mut buf,
                Some(&mut srcmap),
            ),
            comments: Some(&self.comments),
        };
        emitter.emit_module(module).unwrap();

        self.source_map()
            .build_source_map_with_config(&srcmap, None, SourceMapConfig {})
            .to_writer(&mut source_map_buffer)
            .unwrap();

        if inline_source_map {
            let mut comment = "//# sourceMappingURL=data:application/json;base64,"
                .to_owned()
                .into_bytes();
            buf.append(&mut comment);

            let mut encoded = general_purpose::URL_SAFE_NO_PAD
                .encode(source_map_buffer.clone())
                .into_bytes();

            buf.append(&mut encoded);
        }

        let s = String::from_utf8_lossy(&buf);

        CodeMapPair {
            code: s.to_string(),
            map: String::from_utf8(source_map_buffer.clone()).unwrap(),
        }
    }

    pub fn source_map(&self) -> Lrc<SourceMap> {
        return self.source_map.clone();
    }
}

fn insert_import(
    parsed_module: &mut Module,
    target_module: &str,
    target_specifier: &str,
    local: &Ident,
) {
    parsed_module.body.insert(
        0,
        ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
            span: Default::default(),
            specifiers: vec![ImportSpecifier::Named(ImportNamedSpecifier {
                span: Default::default(),
                local: local.clone(),
                imported: Some(ModuleExportName::Ident(Ident::new(
                    target_specifier.into(),
                    Default::default(),
                ))),
                is_type_only: false,
            })],
            src: Box::new(target_module.into()),
            type_only: false,
            with: None,
        })),
    );
}

#[cfg(test)]
mod test_helpers;

macro_rules! testcase {
    ($test_name:ident, $input:expr, $expected:expr) => {
        #[test]
        fn $test_name() -> Result<(), swc_ecma_parser::error::Error> {
            test_helpers::testcase($input, $expected)
        }
    };
}

testcase! {
  no_preexisting_import,
  r#"let x = <template>hello</template>"#,
  r#"import { template as template_UUID } from "@ember/template-compiler";
     let x = template_UUID(`hello`, { eval() { return eval(arguments[0])} });"#
}

testcase! {
  preexisting_import,
  r#"import { template } from "@ember/template-compiler";
     let x = <template>hello</template>"#,
  r#"import { template as template_UUID } from "@ember/template-compiler";
     import { template } from "@ember/template-compiler";
     let x = template_UUID(`hello`, { eval() { return eval(arguments[0])} });"#
}

testcase! {
  preexisting_renamed_import,
  r#"import { template as t } from "@ember/template-compiler";
     let x = <template>hello</template>"#,
  r#"import { template as template_UUID } from "@ember/template-compiler";
     import { template as t } from "@ember/template-compiler";
     let x = template_UUID(`hello`, { eval() { return eval(arguments[0])} })"#
}

testcase! {
  no_template_tags,
  r#"console.log('hello')"#,
  r#"console.log('hello')"#
}

testcase! {
  avoids_top_level_collision,
  r#"function template() {};
     console.log(template());
     export default <template>Hi</template>"#,
  r#"import { template as template_UUID } from "@ember/template-compiler";
     function template() {};
     console.log(template());
     export default template_UUID(`Hi`, { eval() { return eval(arguments[0])} });"#
}

testcase! {
  avoids_local_collision,
  r#"export default function (template) {
         console.log(template);
         return <template>X</template>;
       };"#,
  r#"import { template as template_UUID } from "@ember/template-compiler";
       export default function(template) {
         console.log(template);
         return template_UUID(`X`, { eval() { return eval(arguments[0])} });
       };"#
}

testcase! {
  handles_typescript,
  r#"function makeComponent(message: string) {
        console.log(message);
        return <template>hello</template>
    }"#,
  r#"import { template as template_UUID } from "@ember/template-compiler";
       function makeComponent(message: string) {
         console.log(message);
         return template_UUID(`hello`, { eval() { return eval(arguments[0]) } });
       }"#
}

testcase! {
  handles_typescript_this,
  r#"function f(this: Context, ...args: unknown[]) {
        function t(this: Context, ...args: unknown[]) {};
        return <template></template>
    }"#,
  r#"import { template as template_UUID } from "@ember/template-compiler";
       function f(this: Context, ...args: unknown[]) {
         function t(this: Context, ...args: unknown[]) {}
         ;
         return template_UUID(``, { eval() { return eval(arguments[0]) } });
       }"#
}

testcase! {
  automatic_export_default,
  r#"<template>Hello world</template>"#,
  r#"import { template } from "@ember/template-compiler";
      export default template(`Hello world`, { eval() { return eval(arguments[0]) } });
   "#
}

testcase! {
  automatic_export_default_typescript,
  r#"<template>Hello world</template> satisfies MyType"#,
  r#"import { template } from "@ember/template-compiler";
      export default template(`Hello world`, { eval() { return eval(arguments[0]) } }) satisfies MyType;
   "#
}
