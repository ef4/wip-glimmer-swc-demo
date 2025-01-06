use swc_core::ecma::{
    ast::{
        BlockStmt, CallExpr, Callee, ClassMember, ContentTagExpression, ContentTagMember, Expr,
        ExprStmt, Ident, StaticBlock, Stmt,
    },
    transforms::testing::test,
    visit::VisitMut,
    visit::VisitMutWith,
};

use swc_ecma_ast::{
    ContentTagContent, ExportDefaultExpr, ExprOrSpread, ModuleDecl, ModuleItem, Tpl, TplElement,
};

use swc_atoms::Atom;

pub struct TransformVisitor<'a> {
    template_identifier: Ident,
    found_it: Option<&'a mut bool>,
}

impl<'a> TransformVisitor<'a> {
    pub fn new(id: &Ident, found_it: Option<&'a mut bool>) -> Self {
        TransformVisitor {
            template_identifier: id.clone(),
            found_it,
        }
    }
    fn set_found_it(&mut self) {
        match self.found_it.as_mut() {
            Some(flag) => **flag = true,
            None => {}
        }
    }
    fn transform_tag_expression(&mut self, expr: &ContentTagExpression) -> Expr {
        let ContentTagExpression {
            span,
            contents,
            closing,
            ..
        } = expr;

        Expr::Call(CallExpr {
            span: *span,
            callee: Callee::Expr(Box::new(Expr::Ident(self.template_identifier.clone()))),
            args: vec![
                self.content_literal(contents),
                crate::snippets::scope_params(closing.span).into(),
            ],
            type_args: None,
            ..Default::default()
        })
    }

    fn content_literal(&self, contents: &Box<ContentTagContent>) -> ExprOrSpread {
        Box::new(Expr::Tpl(Tpl {
            span: contents.span,
            exprs: vec![],
            quasis: vec![TplElement {
                span: contents.span,
                cooked: None,
                raw: escape_template_literal(&contents.value),
                tail: false,
            }],
        }))
        .into()
    }
}

fn escape_template_literal(input: &Atom) -> Atom {
    input
        .replace("\\", "\\\\")
        .replace("`", "\\`")
        .replace("$", "\\$")
        .into()
}

impl<'a> VisitMut for TransformVisitor<'a> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        n.visit_mut_children_with(self);
        if let Expr::ContentTagExpression(expr) = n {
            *n = self.transform_tag_expression(expr);
            self.set_found_it();
        }
    }

    fn visit_mut_class_member(&mut self, n: &mut ClassMember) {
        n.visit_mut_children_with(self);
        if let ClassMember::ContentTagMember(ContentTagMember {
            span,
            opening,
            contents,
            closing,
        }) = n
        {
            let call_expr = Expr::Call(CallExpr {
                span: *span,
                callee: Callee::Expr(Box::new(Expr::Ident(self.template_identifier.clone()))),
                args: vec![
                    self.content_literal(contents),
                    crate::snippets::scope_params_with_this(closing.span).into(),
                ],
                type_args: None,
                ..Default::default()
            });
            let call_statement = ExprStmt {
                span: *span,
                expr: Box::new(call_expr),
            };
            *n = ClassMember::StaticBlock(StaticBlock {
                span: opening.span,
                body: BlockStmt {
                    span: *span,
                    stmts: vec![Stmt::Expr(call_statement)],
                    ..Default::default()
                },
            });
            self.set_found_it();
        }
    }

    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        let mut items_updated = Vec::with_capacity(items.len());
        for item in items.drain(..) {
            if let Some(content_tag) = content_tag_expression_statement(&item) {
                items_updated.push(ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(
                    ExportDefaultExpr {
                        span: content_tag.span,
                        expr: Box::new(self.transform_tag_expression(&content_tag)),
                    },
                )));
                self.set_found_it();
            } else {
                items_updated.push(item);
            }
        }

        *items = items_updated;
        items.visit_mut_children_with(self)
    }
}

fn content_tag_expression_statement(item: &ModuleItem) -> Option<&ContentTagExpression> {
    if let ModuleItem::Stmt(Stmt::Expr(ExprStmt {
        expr: box Expr::ContentTagExpression(content_tag),
        ..
    })) = item
    {
        Some(content_tag)
    } else {
        None
    }
}

#[cfg(test)]
use swc_core::ecma::visit::visit_mut_pass;

test!(
    Default::default(),
    |_| {
        visit_mut_pass(TransformVisitor::new(
            &Ident::new("template".into(), Default::default(), Default::default()),
            None,
        ))
    },
    content_tag_template_expression,
    r#"let x = <template>Hello</template>"#
);

test!(
    Default::default(),
    |_| visit_mut_pass(TransformVisitor::new(
        &Ident::new("template".into(), Default::default(), Default::default()),
        None,
    )),
    content_tag_template_member,
    r#"class X { <template>Hello</template> } "#
);

test!(
    Default::default(),
    |_| visit_mut_pass(TransformVisitor::new(
        &Ident::new("template".into(), Default::default(), Default::default()),
        None,
    )),
    expression_inside_class_member,
    r#"class X { thing = <template>Hello</template> } "#
);

test!(
    Default::default(),
    |_| visit_mut_pass(TransformVisitor::new(
        &Ident::new("template".into(), Default::default(), Default::default()),
        None,
    )),
    class_member_inside_expression,
    r#"let x = class { <template>Hello</template> } "#
);

test!(
    Default::default(),
    |_| visit_mut_pass(TransformVisitor::new(
        &Ident::new("template".into(), Default::default(), Default::default()),
        None,
    )),
    content_tag_export_default,
    r#"<template>Hello</template>"#
);

test!(
    Default::default(),
    |_| visit_mut_pass(TransformVisitor::new(
        &Ident::new("template".into(), Default::default(), Default::default()),
        None,
    )),
    inner_expression,
    r#"let x = doIt(<template>Hello</template>)"#
);

test!(
    Default::default(),
    |_| visit_mut_pass(TransformVisitor::new(
        &Ident::new("template".into(), Default::default(), Default::default()),
        None,
    )),
    backtick_in_template,
    r#"let x = <template>He`llo</template>"#
);

test!(
    Default::default(),
    |_| visit_mut_pass(TransformVisitor::new(
        &Ident::new("template".into(), Default::default(), Default::default()),
        None,
    )),
    dollar_in_template,
    r#"let x = <template>He${ll}o</template>"#
);

test!(
    Default::default(),
    |_| visit_mut_pass(TransformVisitor::new(
        &Ident::new("template".into(), Default::default(), Default::default()),
        None,
    )),
    do_not_interpret_js_escapes_in_hbs,
    r#"let x = <template>Hello\nWorld\u1234</template>"#
);
