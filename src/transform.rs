use swc_core::ecma::{
    ast::{
        BlockStmt, CallExpr, Callee, ClassMember, Expr, ExprOrSpread, ExprStmt,
        GlimmerTemplateExpression, GlimmerTemplateMember, Lit, StaticBlock, Stmt, ThisExpr, 
    },
    transforms::testing::test,
    visit::VisitMut,
};

use swc_ecma_ast::Ident;

pub struct TransformVisitor<'a> {
    template_identifier: Ident,
    found_it: Option<&'a mut bool>,
}

impl <'a> TransformVisitor<'a> {
    pub fn new(id: &Ident, found_it: Option<&'a mut bool>) -> Self {
        TransformVisitor { template_identifier: id.clone(), found_it }
    }
    fn set_found_it(&mut self)  {
        match self.found_it.as_mut() {
            Some(flag) => { **flag = true },
            None => {}
        }
    }
}

impl <'a> VisitMut for TransformVisitor<'a> {
    fn visit_mut_expr(&mut self, n: &mut Expr) {
        if let Expr::GlimmerTemplateExpression(GlimmerTemplateExpression { span, contents }) = n {
            let content_literal = ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(contents.clone().into()))),
            };
            *n = Expr::Call(CallExpr {
                span: *span,
                callee: Callee::Expr(Box::new(Expr::Ident(self.template_identifier.clone()))),
                args: vec![content_literal],
                type_args: None,
            });
            self.set_found_it();
        }
    }

    fn visit_mut_class_member(&mut self, n: &mut ClassMember) {
        if let ClassMember::GlimmerTemplateMember(GlimmerTemplateMember { span, contents }) = n {
            let content_literal = ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(contents.clone().into()))),
            };
            let this = ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::This(ThisExpr { span: *span })),
            };
            let call_expr = Expr::Call(CallExpr {
                span: *span,
                callee: Callee::Expr(Box::new(Expr::Ident(self.template_identifier.clone()))),
                args: vec![content_literal, this],
                type_args: None,
            });
            let call_statement = ExprStmt {
                span: *span,
                expr: Box::new(call_expr),
            };
            *n = ClassMember::StaticBlock(StaticBlock {
                span: *span,
                body: BlockStmt {
                    span: *span,
                    stmts: vec![Stmt::Expr(call_statement)],
                },
            });
            self.set_found_it();
        }
    }
}

#[cfg(test)]
use swc_core::ecma::visit::as_folder;

test!(
    Default::default(),
    |_| {
        as_folder(TransformVisitor::new(&Ident::new("template".into(), Default::default()), None))
    },
    glimmer_template_expression,
    r#"let x = <template>Hello</template>"#,
    r#"let x = template("Hello")"#
);

test!(
    Default::default(),
    |_| as_folder(TransformVisitor::new(&Ident::new("template".into(), Default::default()), None)),
    glimmer_template_member,
    r#"class X { <template>Hello</template> } "#,
    r#"class X {
      static {
          template("Hello", this);
      }
  }"#
);

