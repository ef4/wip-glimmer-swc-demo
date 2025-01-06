use swc_common::comments::SingleThreadedComments;
use swc_common::Span;
use swc_common::{self, sync::Lrc, FileName, SourceMap};
use swc_ecma_ast::{Expr, Module};
use swc_ecma_parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
use swc_ecma_visit::{visit_mut_pass, VisitMut, VisitMutWith};

lazy_static! {
    static ref SCOPE_PARAMS: Module = parse(r#"({ eval() { return eval(arguments[0]); } })"#);
}

lazy_static! {
    static ref SCOPE_PARAMS_WITH_THIS: Module =
        parse(r#"({ component: this, eval() { return eval(arguments[0]); } })"#);
}

fn parse(src: &str) -> Module {
    let filename = "glimmer-template-prelude.js".into();
    let source_map: Lrc<SourceMap> = Default::default();
    let comments = SingleThreadedComments::default();

    let source_file = source_map.new_source_file(FileName::Real(filename).into(), src.to_string());

    let lexer = Lexer::new(
        Syntax::Es(EsSyntax {
            decorators: true,
            ..Default::default()
        }),
        Default::default(),
        StringInput::from(&*source_file),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);
    parser.parse_module().unwrap()
}

struct SpanReplacer {
    span: Span,
}
impl VisitMut for SpanReplacer {
    fn visit_mut_span(&mut self, n: &mut Span) {
        *n = self.span;
    }
}

fn generate_expression(span: Span, template_module: &Module) -> Box<Expr> {
    let mut module = template_module.clone();
    module.visit_mut_with(&mut visit_mut_pass(SpanReplacer { span }));
    module
        .body
        .first()
        .unwrap()
        .as_stmt()
        .unwrap()
        .as_expr()
        .unwrap()
        .expr
        .as_paren()
        .unwrap()
        .expr
        .clone()
}

pub fn scope_params(span: Span) -> Box<Expr> {
    generate_expression(span, &(*SCOPE_PARAMS))
}

pub fn scope_params_with_this(span: Span) -> Box<Expr> {
    generate_expression(span, &(*SCOPE_PARAMS_WITH_THIS))
}
