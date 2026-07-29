//! Canonical AST pretty-printer for Covenant source files.
//!
//! The output is guaranteed to be idempotent:
//!   `pretty_print(parse(pretty_print(parse(src)))) == pretty_print(parse(src))`
//!
//! **Comments are not preserved**: the lexer discards them during tokenisation.

use covenant_lexer::DurationUnit;

use crate::ast::*;

const INDENT: &str = "    ";

/// Pretty-print a parsed [`File`] back into Covenant source text.
pub fn pretty_print(file: &File) -> String {
    let mut p = Printer::new();
    p.print_file(file);
    p.finish()
}

// ---------------------------------------------------------------------------
// Printer state
// ---------------------------------------------------------------------------

struct Printer {
    out: String,
    depth: usize,
}

impl Printer {
    fn new() -> Self {
        Self {
            out: String::new(),
            depth: 0,
        }
    }

    fn finish(self) -> String {
        self.out
    }

    fn ind(&self) -> String {
        INDENT.repeat(self.depth)
    }

    /// Write one line at current indentation (adds trailing newline).
    fn line(&mut self, s: &str) {
        let ind = self.ind();
        self.out.push_str(&ind);
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn raw(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn newline(&mut self) {
        self.out.push('\n');
    }

    // -----------------------------------------------------------------------

    fn print_file(&mut self, file: &File) {
        if let Some(h) = &file.header {
            self.raw(&format!("covenant {}\n\n", h.covenant_version));
        }
        for imp in &file.imports {
            let path: Vec<&str> = imp.path.iter().map(|i| i.name.as_ref()).collect();
            let mut s = format!("import {}", path.join("::"));
            if let Some(alias) = &imp.alias {
                s.push_str(&format!(" as {}", alias.name));
            }
            self.raw(&s);
            self.newline();
        }
        if !file.imports.is_empty() {
            self.newline();
        }
        for ec in &file.external_contracts {
            self.print_external_contract(ec);
            self.newline();
        }
        self.print_top_level(&file.top_level);
    }

    fn print_external_contract(&mut self, ec: &ExternalContractDecl) {
        self.raw(&format!("external contract {} {{\n", ec.name.name));
        self.depth += 1;
        for f in &ec.functions {
            self.print_external_function(f);
        }
        self.depth -= 1;
        self.line("}");
    }

    fn print_external_function(&mut self, f: &ExternalFunctionDecl) {
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| {
                if let Some(n) = &p.name {
                    format!("{} {}", type_str(&p.ty), n.name)
                } else {
                    type_str(&p.ty)
                }
            })
            .collect();
        let mut s = format!("function {}({})", f.name.name, params.join(", "));
        if f.is_view {
            s.push_str(" view");
        }
        if let Some(ret) = &f.returns {
            s.push_str(&format!(" returns {}", type_str(ret)));
        }
        self.line(&s);
    }

    fn print_top_level(&mut self, tl: &TopLevel) {
        if let Some(pq) = &tl.privacy {
            self.raw(&format!("{} ", privacy_qual_str(pq)));
        }
        self.raw(&format!(
            "{} {}",
            construct_kw_str(tl.keyword),
            tl.name.name
        ));
        if let Some(anchor) = &tl.anchor {
            let chains: Vec<&str> = anchor.chains.iter().map(|c| c.value.as_ref()).collect();
            self.raw(&format!(" anchored_on {}", chains.join(", ")));
        }
        if let Some(ug) = &tl.upgradeable {
            self.raw(&format!(" upgradeable_by {}", principal_str(&ug.principal)));
        }
        self.raw(" {\n");
        self.depth += 1;

        let mut first = true;
        for decl in &tl.body {
            let needs_blank = !first
                && matches!(
                    decl,
                    TopLevelDecl::Action(_)
                        | TopLevelDecl::View(_)
                        | TopLevelDecl::Reveal(_)
                        | TopLevelDecl::Struct(_)
                        | TopLevelDecl::Credential(_)
                        | TopLevelDecl::Version(_)
                        | TopLevelDecl::Migrate(_)
                        | TopLevelDecl::OnDestroy(_)
                        | TopLevelDecl::SelectiveDisclosure(_)
                );
            if needs_blank {
                self.newline();
            }
            self.print_top_level_decl(decl);
            first = false;
        }

        self.depth -= 1;
        self.raw("}\n");
    }

    fn print_top_level_decl(&mut self, decl: &TopLevelDecl) {
        match decl {
            TopLevelDecl::Metadata(m) => self.print_metadata(m),
            TopLevelDecl::Field(f) => self.print_field_decl(f),
            TopLevelDecl::Struct(s) => self.print_struct_decl(s),
            TopLevelDecl::Credential(c) => self.print_credential_decl(c),
            TopLevelDecl::Error(e) => self.print_error_decl(e),
            TopLevelDecl::Event(e) => self.print_event_decl(e),
            TopLevelDecl::Action(a) => self.print_action_decl(a),
            TopLevelDecl::View(v) => self.print_view_decl(v),
            TopLevelDecl::Reveal(r) => self.print_reveal_decl(r),
            TopLevelDecl::Version(v) => self.print_version_block(v),
            TopLevelDecl::Migrate(m) => self.print_migrate_block(m),
            TopLevelDecl::OnDestroy(o) => self.print_on_destroy_block(o),
            TopLevelDecl::SelectiveDisclosure(s) => self.print_selective_disclosure(s),
        }
    }

    fn print_metadata(&mut self, m: &MetadataAssignment) {
        let val = match &m.value {
            MetadataValue::Literal(e) => expr_str(e),
            MetadataValue::GenesisMint { amount, to } => {
                format!("{} to {}", expr_str(amount), principal_str(to))
            }
        };
        self.line(&format!("{}: {}", m.name.name, val));
    }

    fn print_field_decl(&mut self, f: &FieldDecl) {
        let mut s = String::new();
        if let Some(pq) = &f.per_field_privacy {
            s.push_str(&format!("{} ", privacy_qual_str(pq)));
        }
        if f.explicit_field_keyword {
            s.push_str("field ");
        }
        s.push_str(&format!("{}: {}", f.name.name, type_str(&f.ty)));
        if let Some(init) = &f.initializer {
            s.push_str(&format!(" = {}", expr_str(init)));
        }
        self.line(&s);
    }

    fn print_struct_decl(&mut self, s: &StructDecl) {
        self.line(&format!("struct {} {{", s.name.name));
        self.depth += 1;
        for f in &s.fields {
            self.print_field_decl(f);
        }
        self.depth -= 1;
        self.line("}");
    }

    fn print_credential_decl(&mut self, c: &CredentialDecl) {
        self.line(&format!("credential {} {{", c.name.name));
        self.depth += 1;
        for f in &c.fields {
            self.print_field_decl(f);
        }
        self.depth -= 1;
        self.line("}");
    }

    fn print_error_decl(&mut self, e: &ErrorDecl) {
        if e.fields.is_empty() {
            self.line(&format!("error {}()", e.name.name));
        } else {
            let fields: Vec<String> = e
                .fields
                .iter()
                .map(|f| format!("{}: {}", f.name.name, type_str(&f.ty)))
                .collect();
            self.line(&format!("error {}({})", e.name.name, fields.join(", ")));
        }
    }

    fn print_event_decl(&mut self, e: &EventDecl) {
        let args: Vec<String> = e
            .args
            .iter()
            .map(|a| {
                if a.indexed {
                    format!("indexed {}: {}", a.name.name, type_str(&a.ty))
                } else {
                    format!("{}: {}", a.name.name, type_str(&a.ty))
                }
            })
            .collect();
        self.line(&format!("event {}({})", e.name.name, args.join(", ")));
    }

    fn print_action_decl(&mut self, a: &ActionDecl) {
        for ann in &a.annotations {
            let s = annotation_str(ann);
            self.line(&s);
        }
        let args_str = fmt_args(&a.args);
        let mut header = format!("action {}({args_str})", a.name.name);
        for (i, g) in a.guards.iter().enumerate() {
            if i > 0 {
                header.push(',');
            }
            header.push(' ');
            header.push_str(&guard_str(g));
        }
        for q in &a.qualifiers {
            header.push(' ');
            header.push_str(&qualifier_str(q));
        }
        header.push_str(" {");
        self.line(&header);
        self.depth += 1;
        for stmt in &a.body {
            let s = stmt_str(stmt, self.depth);
            self.raw(&s);
        }
        self.depth -= 1;
        self.line("}");
    }

    fn print_view_decl(&mut self, v: &ViewDecl) {
        let args_str = fmt_args(&v.args);
        let mut header = format!("view {}({args_str})", v.name.name);
        if let Some(ret) = &v.returns {
            header.push_str(&format!(" returns {}", type_str(ret)));
        }
        for g in &v.guards {
            header.push(' ');
            header.push_str(&guard_str(g));
        }
        let body = expr_str(&v.body);
        header.push_str(&format!(" {{ {body} }}"));
        self.line(&header);
    }

    fn print_reveal_decl(&mut self, r: &RevealDecl) {
        // One-liner form: `reveal name to target` (no args, no body)
        if r.args.is_empty() && r.body.is_none() {
            let target = r
                .target
                .as_ref()
                .map(|t| format!(" to {}", reveal_target_str(t)))
                .unwrap_or_default();
            self.line(&format!("reveal {}{}", r.name.name, target));
            return;
        }
        // Full form
        let args_str = fmt_args(&r.args);
        let mut header = format!("reveal {}({args_str})", r.name.name);
        if let Some(ret) = &r.returns {
            header.push_str(&format!(" returns {}", type_str(ret)));
        }
        if let Some(target) = &r.target {
            header.push_str(&format!(" to {}", reveal_target_str(target)));
        }
        for (i, g) in r.guards.iter().enumerate() {
            if i > 0 {
                header.push(',');
            }
            header.push(' ');
            header.push_str(&guard_str(g));
        }
        if let Some(body) = &r.body {
            header.push_str(&format!(" {{ {} }}", expr_str(body)));
        }
        self.line(&header);
    }

    fn print_version_block(&mut self, v: &VersionBlock) {
        self.line(&format!("version {} {{", v.number));
        self.depth += 1;
        for decl in &v.body {
            self.print_top_level_decl(decl);
        }
        self.depth -= 1;
        self.line("}");
    }

    fn print_migrate_block(&mut self, m: &MigrateBlock) {
        self.line(&format!("migrate from {} to {} {{", m.from, m.to));
        self.depth += 1;
        for stmt in &m.body {
            let s = stmt_str(stmt, self.depth);
            self.raw(&s);
        }
        self.depth -= 1;
        self.line("}");
    }

    fn print_on_destroy_block(&mut self, o: &OnDestroyBlock) {
        self.line("on_destroy {");
        self.depth += 1;
        for stmt in &o.body {
            let s = stmt_str(stmt, self.depth);
            self.raw(&s);
        }
        self.depth -= 1;
        self.line("}");
    }

    fn print_selective_disclosure(&mut self, s: &SelectiveDisclosureDecl) {
        let args_str = fmt_args(&s.args);
        let body = expr_str(&s.body);
        self.line(&format!(
            "selective_disclosure {}({args_str}) using {} {{ {body} }}",
            s.name.name, s.circuit_name.name
        ));
    }
}

// ---------------------------------------------------------------------------
// Statement printing (returns a String with trailing newline, depth-aware)
// ---------------------------------------------------------------------------

fn stmt_str(stmt: &Stmt, depth: usize) -> String {
    let ind = INDENT.repeat(depth);
    let ind1 = INDENT.repeat(depth + 1);
    match stmt {
        Stmt::Let {
            name, ty, value, ..
        } => {
            let ty_part = ty
                .as_ref()
                .map(|t| format!(": {}", type_str(t)))
                .unwrap_or_default();
            format!("{ind}let {}{ty_part} = {}\n", name.name, expr_str(value))
        }
        Stmt::Assign {
            target, op, value, ..
        } => {
            format!(
                "{ind}{} {} {}\n",
                lvalue_str(target),
                assign_op_str(*op),
                expr_str(value)
            )
        }
        Stmt::Expr(e, _) => {
            format!("{ind}{}\n", expr_str(e))
        }
        Stmt::Match {
            scrutinee, arms, ..
        } => {
            let mut s = format!("{ind}match {} {{\n", expr_str(scrutinee));
            for arm in arms {
                let pat = match &arm.pattern {
                    MatchPattern::Literal(e) => expr_str(e),
                };
                s.push_str(&format!("{ind1}{pat} => {{\n"));
                match &arm.body {
                    MatchBody::Stmt(st) => {
                        s.push_str(&stmt_str(st, depth + 2));
                    }
                    MatchBody::Block(stmts) => {
                        for st in stmts {
                            s.push_str(&stmt_str(st, depth + 2));
                        }
                    }
                }
                s.push_str(&format!("{ind1}}}\n"));
            }
            s.push_str(&format!("{ind}}}\n"));
            s
        }
        Stmt::If {
            cond,
            then_block,
            else_ifs,
            else_block,
            ..
        } => {
            let mut s = format!("{ind}if {} {{\n", expr_str(cond));
            for st in then_block {
                s.push_str(&stmt_str(st, depth + 1));
            }
            for (elif_cond, elif_body) in else_ifs {
                s.push_str(&format!("{ind}}} else if {} {{\n", expr_str(elif_cond)));
                for st in elif_body {
                    s.push_str(&stmt_str(st, depth + 1));
                }
            }
            if let Some(else_body) = else_block {
                s.push_str(&format!("{ind}}} else {{\n"));
                for st in else_body {
                    s.push_str(&stmt_str(st, depth + 1));
                }
            }
            s.push_str(&format!("{ind}}}\n"));
            s
        }
        Stmt::EncryptedWhen {
            cond,
            then_block,
            otherwise,
            ..
        } => {
            let mut s = format!("{ind}encrypted_when {} {{\n", expr_str(cond));
            for st in then_block {
                s.push_str(&stmt_str(st, depth + 1));
            }
            if let Some(other) = otherwise {
                s.push_str(&format!("{ind}}} otherwise {{\n"));
                for st in other {
                    s.push_str(&stmt_str(st, depth + 1));
                }
            }
            s.push_str(&format!("{ind}}}\n"));
            s
        }
        Stmt::ForEach {
            binding,
            iter,
            body,
            ..
        } => {
            let mut s = format!("{ind}for {} in {} {{\n", binding.name, expr_str(iter));
            for st in body {
                s.push_str(&stmt_str(st, depth + 1));
            }
            s.push_str(&format!("{ind}}}\n"));
            s
        }
        Stmt::TryCatch {
            body,
            error_binding,
            catch_body,
            ..
        } => {
            let mut s = format!("{ind}try_action {{\n");
            for st in body {
                s.push_str(&stmt_str(st, depth + 1));
            }
            let binding = error_binding
                .as_ref()
                .map(|i| format!(" {}", i.name))
                .unwrap_or_default();
            s.push_str(&format!("{ind}}} catch{binding} {{\n"));
            for st in catch_body {
                s.push_str(&stmt_str(st, depth + 1));
            }
            s.push_str(&format!("{ind}}}\n"));
            s
        }
        Stmt::Emit { event, args, .. } => {
            let arg_strs: Vec<String> = args.iter().map(expr_str).collect();
            format!("{ind}emit {}({})\n", event.name, arg_strs.join(", "))
        }
        Stmt::Transfer {
            amount, from, to, ..
        } => {
            let from_part = from
                .as_ref()
                .map(|f| format!(" from {}", expr_str(f)))
                .unwrap_or_default();
            format!(
                "{ind}transfer {}{from_part} to {}\n",
                expr_str(amount),
                expr_str(to)
            )
        }
        Stmt::Append {
            collection,
            struct_lit,
            ..
        } => {
            let mut s = format!("{ind}append {} {{\n", collection.name);
            for fa in struct_lit {
                s.push_str(&format!(
                    "{ind1}{}: {}\n",
                    fa.name.name,
                    expr_str(&fa.value)
                ));
            }
            s.push_str(&format!("{ind}}}\n"));
            s
        }
        Stmt::Discard(lv, _) => format!("{ind}discard {}\n", lvalue_str(lv)),
        Stmt::Delete(lv, _) => format!("{ind}delete {}\n", lvalue_str(lv)),
        Stmt::Return(None, _) => format!("{ind}return\n"),
        Stmt::Return(Some(e), _) => format!("{ind}return {}\n", expr_str(e)),
        Stmt::RevertWith { error, args, .. } => {
            let arg_strs: Vec<String> = args.iter().map(expr_str).collect();
            format!("{ind}revert_with {}({})\n", error.name, arg_strs.join(", "))
        }
    }
}

// ---------------------------------------------------------------------------
// Expression, type, and misc helpers
// ---------------------------------------------------------------------------

pub fn expr_str(e: &Expr) -> String {
    match e {
        Expr::Literal(l) => literal_str(l),
        Expr::Ident(i) => i.name.to_string(),
        Expr::Binary { op, lhs, rhs, .. } => {
            format!("{} {} {}", expr_str(lhs), binary_op_str(*op), expr_str(rhs))
        }
        Expr::Unary { op, operand, .. } => match op {
            UnaryOp::Not => format!("not {}", expr_str(operand)),
            UnaryOp::Neg => format!("-{}", expr_str(operand)),
            UnaryOp::BitNot => format!("~{}", expr_str(operand)),
        },
        Expr::Call { callee, args, .. } => {
            let arg_strs: Vec<String> = args.iter().map(expr_str).collect();
            format!("{}({})", expr_str(callee), arg_strs.join(", "))
        }
        Expr::FieldAccess { base, field, .. } => {
            format!("{}.{}", expr_str(base), field.name)
        }
        Expr::Index { base, index, .. } => {
            format!("{}[{}]", expr_str(base), expr_str(index))
        }
        Expr::Slice { base, from, to, .. } => {
            format!("{}[{}..{}]", expr_str(base), expr_str(from), expr_str(to))
        }
        Expr::Array { elements, .. } => {
            let el: Vec<String> = elements.iter().map(expr_str).collect();
            format!("[{}]", el.join(", "))
        }
        Expr::Paren(inner) => {
            format!("({})", expr_str(inner))
        }
        Expr::If {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            format!(
                "if {} {{ {} }} else {{ {} }}",
                expr_str(cond),
                expr_str(then_expr),
                expr_str(else_expr)
            )
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let arm_strs: Vec<String> = arms
                .iter()
                .map(|arm| {
                    let pat = match &arm.pattern {
                        MatchPattern::Literal(e) => expr_str(e),
                    };
                    format!("{pat} => {}", expr_str(&arm.body))
                })
                .collect();
            format!(
                "match {} {{ {} }}",
                expr_str(scrutinee),
                arm_strs.join(", ")
            )
        }
        Expr::EncryptedLit(inner, _) => {
            format!("encrypted({})", expr_str(inner))
        }
        Expr::Lambda { param, body, .. } => {
            format!("|{}| {}", param.name, expr_str(body))
        }
    }
}

pub fn type_str(t: &Type) -> String {
    match t {
        Type::Amount(_) => "amount".to_string(),
        Type::Time(_) => "time".to_string(),
        Type::Duration(_) => "duration".to_string(),
        Type::Hash(_) => "hash".to_string(),
        Type::Text(_) => "text".to_string(),
        Type::Address(_) => "address".to_string(),
        Type::Bool(_) => "bool".to_string(),
        Type::Bytes(_) => "bytes".to_string(),
        Type::PqKey(_) => "pq_key".to_string(),
        Type::Encrypted(inner, _) => format!("encrypted<{}>", type_str(inner)),
        // [choice] uses bracket syntax; list<T> for all other element types.
        Type::List(inner, _) if matches!(inner.as_ref(), Type::Choice(_, _)) => {
            format!("[{}]", type_str(inner))
        }
        Type::List(inner, _) => format!("list<{}>", type_str(inner)),
        Type::Map(k, v, _) => format!("map<{}, {}>", type_str(k), type_str(v)),
        Type::PriorityQueue(k, v, ord, _) => {
            let ord_str = match ord {
                Ordering::Max => "max",
                Ordering::Min => "min",
            };
            format!(
                "priority_queue<{}, {}, {ord_str}>",
                type_str(k),
                type_str(v)
            )
        }
        Type::Shares { n, k, parties, .. } => {
            format!("shares({n}/{k}, {})", expr_str(parties))
        }
        Type::Choice(_, vals) => {
            if vals.is_empty() {
                return "choice".to_string();
            }
            let names: Vec<&str> = vals.iter().map(|v| v.name.name.as_ref()).collect();
            format!("choice<{}>", names.join(" | "))
        }
        Type::User(ident) => ident.name.to_string(),
    }
}

fn literal_str(l: &LiteralExpr) -> String {
    match l {
        LiteralExpr::Integer(n, _) => n.to_string(),
        LiteralExpr::Hex(bytes, _) => format!("0x{}", bytes_to_hex(bytes)),
        LiteralExpr::Text(s, _) => format!("\"{}\"", escape_text(s)),
        LiteralExpr::Bool(b, _) => if *b { "true" } else { "false" }.to_string(),
        // `n` is the value in seconds, not the count the author wrote (see
        // `TokenKind::Duration`). Divide by the unit to print the literal back
        // exactly as it appeared: the division is exact because the lexer built
        // `n` as `written * unit.seconds()`.
        LiteralExpr::Duration(n, unit, _) => {
            format!("{} {}", n / unit.seconds(), duration_unit_str(*unit))
        }
    }
}

fn principal_str(p: &Principal) -> String {
    match p {
        Principal::Named(k) => principal_kind_str(*k).to_string(),
        Principal::Predicate(ident) => ident.name.to_string(),
        Principal::Address(e) => expr_str(e),
        Principal::Call { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(expr_str).collect();
            format!("{}({})", name.name, arg_strs.join(", "))
        }
    }
}

fn lvalue_str(lv: &LValue) -> String {
    match lv {
        LValue::Ident(i) => i.name.to_string(),
        LValue::FieldAccess(base, field) => format!("{}.{}", lvalue_str(base), field.name),
        LValue::Index(base, idx) => format!("{}[{}]", lvalue_str(base), expr_str(idx)),
    }
}

fn guard_str(g: &Guard) -> String {
    match g {
        Guard::When(e) => format!("when {}", expr_str(e)),
        Guard::Only(p) => format!("only {}", principal_str(p)),
        Guard::Given(e) => format!("given {}", expr_str(e)),
    }
}

fn qualifier_str(q: &ActionQualifier) -> String {
    match q {
        ActionQualifier::VerifiedBy(e) => format!("verified_by {}", expr_str(e)),
        ActionQualifier::PqSigned { msg, sig, pk, .. } => {
            format!(
                "pq_signed({}, {}, {})",
                expr_str(msg),
                expr_str(sig),
                expr_str(pk)
            )
        }
        ActionQualifier::VdfLocked { delay, .. } => format!("vdf_locked({})", expr_str(delay)),
    }
}

fn annotation_str(a: &Annotation) -> String {
    if a.args.is_empty() {
        return format!("@{}", a.name.name);
    }
    let args: Vec<String> = a
        .args
        .iter()
        .map(|arg| match arg {
            AnnotationArg::Positional(e) => expr_str(e),
            AnnotationArg::Named { name, value } => format!("{}: {}", name.name, expr_str(value)),
        })
        .collect();
    format!("@{}({})", a.name.name, args.join(", "))
}

fn reveal_target_str(t: &RevealTarget) -> String {
    match t {
        RevealTarget::Owner => "owner".to_string(),
        RevealTarget::Caller => "caller".to_string(),
        RevealTarget::Parties => "parties".to_string(),
        RevealTarget::Address(e) => expr_str(e),
    }
}

fn fmt_args(args: &[Arg]) -> String {
    args.iter()
        .map(|a| format!("{}: {}", a.name.name, type_str(&a.ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Keyword string helpers
// ---------------------------------------------------------------------------

fn privacy_qual_str(pq: &PrivacyQualifier) -> &'static str {
    match pq {
        PrivacyQualifier::Public => "public",
        PrivacyQualifier::Private => "private",
        PrivacyQualifier::Encrypted => "encrypted",
        PrivacyQualifier::Sealed => "sealed",
        PrivacyQualifier::Hybrid => "hybrid",
        PrivacyQualifier::Confidential => "confidential",
    }
}

fn construct_kw_str(k: ConstructKind) -> &'static str {
    match k {
        ConstructKind::Module => "module",
        ConstructKind::Record => "record",
        ConstructKind::Token => "token",
        ConstructKind::Ballot => "ballot",
        ConstructKind::Counter => "counter",
        ConstructKind::Board => "board",
        ConstructKind::Market => "market",
        ConstructKind::Vault => "vault",
        ConstructKind::Registry => "registry",
        ConstructKind::Bridge => "bridge",
        ConstructKind::Ceremony => "ceremony",
        ConstructKind::Nft => "nft",
        ConstructKind::Test => "test",
    }
}

fn principal_kind_str(k: PrincipalKind) -> &'static str {
    match k {
        PrincipalKind::Owner => "owner",
        PrincipalKind::Deployer => "deployer",
        PrincipalKind::Admin => "admin",
        PrincipalKind::Caller => "caller",
        PrincipalKind::Guardians => "guardians",
        PrincipalKind::Parties => "parties",
        PrincipalKind::Holders => "holders",
    }
}

fn binary_op_str(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Eq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::LtEq => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::GtEq => ">=",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::Concat => "++",
        BinaryOp::In => "in",
    }
}

fn assign_op_str(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Eq => "=",
        AssignOp::PlusEq => "+=",
        AssignOp::MinusEq => "-=",
        AssignOp::StarEq => "*=",
        AssignOp::SlashEq => "/=",
        AssignOp::PercentEq => "%=",
    }
}

fn duration_unit_str(unit: DurationUnit) -> &'static str {
    match unit {
        DurationUnit::Seconds => "seconds",
        DurationUnit::Minutes => "minutes",
        DurationUnit::Hours => "hours",
        DurationUnit::Days => "days",
        DurationUnit::Weeks => "weeks",
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            write!(s, "{b:02x}").unwrap();
            s
        })
}

fn escape_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use covenant_diag::SourceId;

    fn round_trip(src: &str) -> String {
        let sid = SourceId::new(0);
        let (tokens, _) = covenant_lexer::tokenize(src, sid);
        let (file, _) = crate::parse(&tokens, sid);
        let file = file.expect("parsed");
        pretty_print(&file)
    }

    fn is_idempotent(src: &str) -> bool {
        let once = round_trip(src);
        let twice = round_trip(&once);
        once == twice
    }

    #[test]
    fn idempotent_simple_record() {
        let src = r#"
record Hello {
    x: amount
    action go() only caller { x = 1 }
    view get returns amount { x }
}
"#;
        assert!(is_idempotent(src), "round-trip not idempotent");
    }

    #[test]
    fn idempotent_token() {
        let src = r#"
token Coin {
    symbol: "COIN"
    name: "Covenant Coin"
    decimals: 18
    supply: 1000000 to deployer
}
"#;
        assert!(is_idempotent(src), "token not idempotent");
    }

    #[test]
    fn idempotent_with_events_and_errors() {
        let src = r#"
module SafeTransfer {
    error InsufficientBalance(required: amount, actual: amount)
    error ZeroAmount()
    event Deposited(who: address, value: amount)
    field balances: map<address, amount>
    action deposit(value: amount) {
        balances[caller] += value
        emit Deposited(caller, value)
    }
    view balance_of(who: address) returns amount { balances[who] }
}
"#;
        assert!(is_idempotent(src), "module not idempotent");
    }

    #[test]
    fn idempotent_privacy_qualifier() {
        let src = r#"
encrypted counter PrivateDAO {
    yes_votes: amount
    action vote_yes() {
        yes_votes += 1
    }
}
"#;
        assert!(is_idempotent(src), "encrypted counter not idempotent");
    }

    #[test]
    fn idempotent_if_else() {
        let src = r#"
record R {
    x: amount
    action go(v: amount) only caller {
        if v > 0 {
            x = v
        } else {
            x = 0
        }
    }
    view get returns amount { x }
}
"#;
        assert!(is_idempotent(src), "if/else not idempotent");
    }

    #[test]
    fn idempotent_for_loop() {
        let src = r#"
record R {
    counts: map<address, amount>
    action tally(voters: list<address>) only caller {
        for v in voters {
            counts[v] += 1
        }
    }
}
"#;
        assert!(is_idempotent(src), "for loop not idempotent");
    }

    #[test]
    fn idempotent_revert_with() {
        let src = r#"
module M {
    error Unauthorized(who: address)
    action go() only caller {
        revert_with Unauthorized(caller)
    }
}
"#;
        assert!(is_idempotent(src), "revert_with not idempotent");
    }

    #[test]
    fn type_str_primitives() {
        use covenant_diag::Span;
        let sid = SourceId::new(0);
        let zero = Span::new(sid, 0, 0);
        assert_eq!(type_str(&Type::Amount(zero)), "amount");
        assert_eq!(type_str(&Type::Time(zero)), "time");
        assert_eq!(type_str(&Type::Bool(zero)), "bool");
        assert_eq!(type_str(&Type::Address(zero)), "address");
        assert_eq!(type_str(&Type::Hash(zero)), "hash");
    }

    #[test]
    fn type_str_complex() {
        use covenant_diag::Span;
        let sid = SourceId::new(0);
        let zero = Span::new(sid, 0, 0);
        let inner = Box::new(Type::Amount(zero));
        assert_eq!(type_str(&Type::List(inner.clone(), zero)), "list<amount>");
        assert_eq!(
            type_str(&Type::Map(
                inner.clone(),
                Box::new(Type::Address(zero)),
                zero
            )),
            "map<amount, address>"
        );
        assert_eq!(type_str(&Type::Encrypted(inner, zero)), "encrypted<amount>");
    }

    #[test]
    fn expr_str_literals() {
        use covenant_diag::Span;
        let sid = SourceId::new(0);
        let zero = Span::new(sid, 0, 0);
        assert_eq!(
            expr_str(&Expr::Literal(LiteralExpr::Integer(42, zero))),
            "42"
        );
        assert_eq!(
            expr_str(&Expr::Literal(LiteralExpr::Bool(true, zero))),
            "true"
        );
        assert_eq!(
            expr_str(&Expr::Literal(LiteralExpr::Text("hello".into(), zero))),
            "\"hello\""
        );
    }

    #[test]
    fn expr_str_binary() {
        use covenant_lexer::tokenize;
        let sid = SourceId::new(0);
        let src =
            r#"record R { action go() only caller { x = a + b } view get returns amount { x } }"#;
        let (tokens, _) = tokenize(src, sid);
        let (file, _) = crate::parse(&tokens, sid);
        assert!(file.is_some(), "parsed");
    }

    #[test]
    fn idempotent_transfer_stmt() {
        let src = r#"
record R {
    action pay(recipient: address, amt: amount) only caller {
        transfer amt to recipient
    }
}
"#;
        assert!(is_idempotent(src));
    }

    #[test]
    fn idempotent_emit_stmt() {
        let src = r#"
record R {
    event Transfer(from: address, to: address, value: amount)
    action go(recipient: address, amt: amount) only caller {
        emit Transfer(caller, recipient, amt)
    }
}
"#;
        assert!(is_idempotent(src));
    }

    #[test]
    fn idempotent_reveal_one_liner() {
        let src = r#"
encrypted counter C {
    secret: amount
    reveal secret to owner
}
"#;
        assert!(is_idempotent(src));
    }

    #[test]
    fn idempotent_when_guard() {
        let src = r#"
record R {
    deadline: time
    action go() when now < deadline {
    }
}
"#;
        assert!(is_idempotent(src));
    }

    #[test]
    fn type_str_choice_no_vals_is_bare_keyword() {
        use covenant_diag::Span;
        let sid = SourceId::new(0);
        let zero = Span::new(sid, 0, 0);
        assert_eq!(type_str(&Type::Choice(zero, vec![])), "choice");
    }

    #[test]
    fn type_str_choice_with_vals_has_angle_brackets() {
        use crate::ast::{ChoiceValue, Ident};
        use covenant_diag::Span;
        let sid = SourceId::new(0);
        let zero = Span::new(sid, 0, 0);
        let make = |s: &str| ChoiceValue {
            name: Ident {
                name: s.into(),
                span: zero,
            },
        };
        assert_eq!(
            type_str(&Type::Choice(zero, vec![make("yes"), make("no")])),
            "choice<yes | no>"
        );
    }

    #[test]
    fn type_str_list_of_choice_uses_bracket_syntax() {
        use covenant_diag::Span;
        let sid = SourceId::new(0);
        let zero = Span::new(sid, 0, 0);
        let inner = Box::new(Type::Choice(zero, vec![]));
        assert_eq!(type_str(&Type::List(inner, zero)), "[choice]");
    }

    #[test]
    fn type_str_list_of_amount_uses_generic_syntax() {
        use covenant_diag::Span;
        let sid = SourceId::new(0);
        let zero = Span::new(sid, 0, 0);
        let inner = Box::new(Type::Amount(zero));
        assert_eq!(type_str(&Type::List(inner, zero)), "list<amount>");
    }

    #[test]
    fn idempotent_multiple_guards_comma_separated() {
        let src = r#"
record R {
    deadline: time
    balance: amount
    action go(amt: amount) when now < deadline, only caller {
        balance = amt
    }
}
"#;
        assert!(
            is_idempotent(src),
            "multi-guard action should be idempotent"
        );
    }

    #[test]
    fn idempotent_ballot_construct() {
        let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
        assert!(is_idempotent(src), "ballot construct should be idempotent");
    }

    #[test]
    fn idempotent_action_three_guards() {
        let src = r#"
record R {
    opened: bool
    deadline: time
    action submit(v: amount) when now < deadline, only caller, when v > 0 {
        opened = true
    }
}
"#;
        assert!(is_idempotent(src), "3-guard action should be idempotent");
    }

    #[test]
    fn guard_str_when_formats_correctly() {
        use crate::ast::{Expr, Ident};
        use covenant_diag::Span;
        let sid = SourceId::new(0);
        let zero = Span::new(sid, 0, 0);
        let ident = Ident {
            name: "x".into(),
            span: zero,
        };
        let e = Expr::Ident(ident);
        let g = Guard::When(e);
        assert_eq!(guard_str(&g), "when x");
    }

    #[test]
    fn guard_str_only_caller_formats_correctly() {
        let g = Guard::Only(Principal::Named(PrincipalKind::Caller));
        assert_eq!(guard_str(&g), "only caller");
    }

    #[test]
    fn idempotent_reveal_with_guard_and_body() {
        let src = r#"
ballot B {
    reveal result returns amount
        when now > 0 {
        42
    }
}
"#;
        assert!(
            is_idempotent(src),
            "reveal with guard+body should be idempotent"
        );
    }

    #[test]
    fn no_panic_on_all_fixtures() {
        let fixtures = [
            include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_06_secret_coin.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_07_private_dao.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_08_amnesia_ceremony.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_09_encrypted_bridge.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_10_hybrid_state.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_11_safe_transfer.cov"),
        ];
        for (i, src) in fixtures.iter().enumerate() {
            let result = std::panic::catch_unwind(|| round_trip(src));
            assert!(result.is_ok(), "fixture {i} panicked during pretty-print");
        }
    }

    #[test]
    fn all_fixtures_idempotent() {
        let fixtures = [
            include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_07_private_dao.cov"),
            include_str!("../../covenant-lexer/tests/fixtures/example_11_safe_transfer.cov"),
        ];
        for (i, src) in fixtures.iter().enumerate() {
            assert!(
                is_idempotent(src),
                "fixture {i} not idempotent after pretty-print"
            );
        }
    }
}
