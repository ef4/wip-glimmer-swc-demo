#![feature(box_patterns)]

#[macro_use]
extern crate lazy_static;

use std::path::PathBuf;

use base64::{Engine as _, engine::general_purpose};
use swc_atoms::Atom;
use swc_common::{self, FileName, SourceMap, sync::Lrc};
use swc_common::comments::SingleThreadedComments;
use swc_common::source_map::SourceMapGenConfig;
use swc_core::common::GLOBALS;
use swc_ecma_ast::{Ident, ImportDecl, ImportNamedSpecifier, ImportSpecifier, Module, ModuleDecl, ModuleExportName, ModuleItem};
use swc_ecma_codegen::Emitter;
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsConfig};
use swc_ecma_utils::private_ident;
use swc_ecma_visit::{as_folder, VisitMut, VisitMutWith, VisitWith};

mod bindings;
mod snippets;
mod transform;
mod locate;

#[derive(Default)]
pub struct Options {
    pub filename: Option<PathBuf>,
    pub inline_source_map: bool,
}

pub struct Preprocessor {
    source_map: Lrc<SourceMap>,
    comments: SingleThreadedComments,
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


pub struct IdentCollector {
  pub idents: Vec<Atom>
}

impl IdentCollector {
  pub fn new() -> Self {
    Self { idents: vec![] }
  }
}

impl VisitMut for IdentCollector {
  fn visit_mut_ident(&mut self, n: &mut Ident) {
    self.idents.push(n.sym.clone())
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
    ) -> Result<String, swc_ecma_parser::error::Error> {
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

            let mut ident_collector = IdentCollector::new();
            parsed_module.visit_mut_with(&mut as_folder(&mut ident_collector));

            let found_id = find_existing_import(&parsed_module, target_module, target_specifier);
            let had_id_already = found_id.is_some();
            let mut new_specifier = String::from(target_specifier);
            if !had_id_already {
              let mut n = 1;
              loop {
                let current = Atom::from(new_specifier.clone());
                let found = ident_collector.idents.contains(&current);
                if found == false {
                  break
                }
                new_specifier = format!("{target_specifier}{n}");
                n = n + 1;
              }
            }
            let id = found_id.unwrap_or_else(|| private_ident!(new_specifier));
            let mut needs_import = false;
            parsed_module.visit_mut_with(&mut as_folder(transform::TransformVisitor::new(
                &id,
                Some(&mut needs_import),
            )));

            if !had_id_already && needs_import {
                insert_import(&mut parsed_module, target_module, target_specifier, &id)
            }

            simplify_imports(&mut parsed_module);

            Ok(self.print(&parsed_module, options.inline_source_map))
        })
    }

    fn print(&self, module: &Module, inline_source_map: bool) -> String {
        let mut buf = vec![];
        let mut srcmap = vec![];
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

        if inline_source_map {
            let mut source_map_buffer = vec![];
            self.source_map()
                .build_source_map_with_config(&srcmap, None, SourceMapConfig {})
                .to_writer(&mut source_map_buffer)
                .unwrap();

            let mut comment = "//# sourceMappingURL=data:application/json;base64,"
                .to_owned()
                .into_bytes();
            buf.append(&mut comment);

            let mut encoded = general_purpose::URL_SAFE_NO_PAD
                .encode(source_map_buffer)
                .into_bytes();

            buf.append(&mut encoded);
        }

        let s = String::from_utf8_lossy(&buf);
        s.to_string()
    }

    pub fn source_map(&self) -> Lrc<SourceMap> {
        return self.source_map.clone();
    }
}

fn find_existing_import(
    parsed_module: &Module,
    target_module: &str,
    target_specifier: &str,
) -> Option<Ident> {
    for item in parsed_module.body.iter() {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(import_declaration)) => {
                if import_declaration.src.value.to_string() == target_module {
                    for specifier in import_declaration.specifiers.iter() {
                        match specifier {
                            ImportSpecifier::Named(s) => {
                                let imported = match &s.imported {
                                    Some(ModuleExportName::Ident(i)) => i.sym.to_string(),
                                    Some(ModuleExportName::Str(s)) => s.value.to_string(),
                                    None => s.local.sym.to_string(),
                                };
                                if imported == target_specifier {
                                    return Some(s.local.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
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
            phase: Default::default(),
        })),
    );
}

// It's not until after the hygiene pass that we know what local name is being
// used for our import. If it turns out to equal the imported name, we can
// implify from "import { template as template } from..." down to  "import {
// template } from ...".
fn simplify_imports(parsed_module: &mut Module) {
    for item in parsed_module.body.iter_mut() {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(import_declaration)) => {
                for specifier in import_declaration.specifiers.iter_mut() {
                    match specifier {
                        ImportSpecifier::Named(specifier) => {
                            if let ImportNamedSpecifier {
                                imported: Some(ModuleExportName::Ident(imported)),
                                local,
                                ..
                            } = specifier
                            {
                                if local.sym == imported.sym {
                                    specifier.imported = None;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
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
  r#"import { template } from "@ember/template-compiler";
     let x = template(`hello`, { eval() { return eval(arguments[0])} });"#
}

testcase! {
  uses_preexisting_import,
  r#"import { template } from "@ember/template-compiler";
     let x = <template>hello</template>"#,
  r#"import { template } from "@ember/template-compiler";
     let x = template(`hello`, { eval() { return eval(arguments[0])} });"#
}

testcase! {
  uses_preexisting_renamed_import,
  r#"import { template as t } from "@ember/template-compiler";
     let x = <template>hello</template>"#,
  r#"import { template as t } from "@ember/template-compiler";
     let x = t(`hello`, { eval() { return eval(arguments[0])} })"#
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
  r#"import { template as template1 } from "@ember/template-compiler";
     function template() {};
     console.log(template());
     export default template1(`Hi`, { eval() { return eval(arguments[0])} });"#
}

testcase! {
  avoids_local_collision,
  r#"export default function (template) {
         console.log(template);
         return <template>X</template>;
       };"#,
  r#"import { template as template1 } from "@ember/template-compiler";
       export default function(template) {
         console.log(template);
         return template1(`X`, { eval() { return eval(arguments[0])} });
       };"#
}

testcase! {
  handles_typescript,
  r#"function makeComponent(message: string) {
        console.log(message);
        return <template>hello</template>
    }"#,
  r#"import { template } from "@ember/template-compiler";
       function makeComponent(message: string) {
         console.log(message);
         return template(`hello`, { eval() { return eval(arguments[0]) } });
       }"#
}

testcase! {
  handles_typescript_this,
  r#"function f(this: Context, ...args: unknown[]) {
        function t(this: Context, ...args: unknown[]) {};
        return <template></template>
    }"#,
  r#"import { template } from "@ember/template-compiler";
       function f(this: Context, ...args: unknown[]) {
         function t(this: Context, ...args: unknown[]) {}
         ;
         return template(``, { eval() { return eval(arguments[0]) } });
       }"#
}
