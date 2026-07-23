//! Operator overloading dispatch tables.
//!
//! Arithmetic, comparison, boolean, concat, in, bitwise, and unary operators.
//! Returns `Some(result_ty)` when the dispatch matches, `None` to signal E202.
//!
//! `Unknown` is treated as a wildcard that matches anything (propagates error
//! suppression). If both operands are concrete, the explicit rule table drives
//! the result.

use covenant_parser::ast::{BinaryOp, UnaryOp};

use crate::ty::Ty;

/// Result of operator dispatch including whether a plaintext operand was
/// auto-lifted to ciphertext (for recording a `LiftMarker`).
pub struct OpResult {
    pub ty: Ty,
    pub lift_lhs: bool,
    pub lift_rhs: bool,
}

impl OpResult {
    fn plain(ty: Ty) -> Self {
        Self {
            ty,
            lift_lhs: false,
            lift_rhs: false,
        }
    }
    fn lift_right(ty: Ty) -> Self {
        Self {
            ty,
            lift_lhs: false,
            lift_rhs: true,
        }
    }
    fn lift_left(ty: Ty) -> Self {
        Self {
            ty,
            lift_lhs: true,
            lift_rhs: false,
        }
    }
}

pub fn dispatch_binary(op: BinaryOp, lhs: &Ty, rhs: &Ty) -> Option<OpResult> {
    // Unknown short-circuit.
    if matches!(lhs, Ty::Unknown) || matches!(rhs, Ty::Unknown) {
        return Some(OpResult::plain(Ty::Unknown));
    }

    use BinaryOp::*;
    use Ty::*;

    // Arithmetic
    if matches!(op, Add | Sub | Mul | Div | Mod) {
        if let Some(r) = arithmetic(op, lhs, rhs) {
            return Some(r);
        }
    }

    // Comparison (ordering + equality)
    if matches!(op, Eq | NotEq | Lt | LtEq | Gt | GtEq) {
        if let Some(r) = comparison(op, lhs, rhs) {
            return Some(r);
        }
    }

    // Boolean
    if matches!(op, And | Or) {
        if let Some(r) = boolean(lhs, rhs) {
            return Some(r);
        }
    }

    // Concat
    if matches!(op, Concat) {
        match (lhs, rhs) {
            (Text, Text) => return Some(OpResult::plain(Text)),
            (Bytes, Bytes) => return Some(OpResult::plain(Bytes)),
            (List(a), List(b)) if a == b => return Some(OpResult::plain(List(a.clone()))),
            _ => {}
        }
    }

    // Membership
    if matches!(op, In) {
        if let List(el) = rhs {
            if *lhs == **el {
                return Some(OpResult::plain(Bool));
            }
            // Allow plaintext-in-list-of-choice with Unknown element fallthrough.
            if matches!(**el, Ty::Unknown) {
                return Some(OpResult::plain(Bool));
            }
            // Phase-4 leniency: a bare `choice` (distinct ChoiceTypeId from
            // the list's element) is still accepted when both sides are
            // Choice-typed. Structural choice equality tightens this later.
            if let (Ty::Choice(_), Ty::Choice(_)) = (lhs, &**el) {
                return Some(OpResult::plain(Bool));
            }
        }
    }

    // Bitwise — plaintext only in V0.
    if matches!(op, BitAnd | BitOr | BitXor | Shl | Shr) {
        match (lhs, rhs) {
            (Amount, Amount) => return Some(OpResult::plain(Amount)),
            (Bytes, Bytes) => return Some(OpResult::plain(Bytes)),
            _ => {}
        }
    }

    None
}

fn arithmetic(op: BinaryOp, lhs: &Ty, rhs: &Ty) -> Option<OpResult> {
    use BinaryOp::*;
    use Ty::*;

    // Plaintext numeric.
    if let (Amount, Amount) = (lhs, rhs) {
        return Some(OpResult::plain(Amount));
    }

    // Time / Duration rules.
    match (op, lhs, rhs) {
        (Add, Time, Duration) | (Add, Duration, Time) => return Some(OpResult::plain(Time)),
        (Sub, Time, Time) => return Some(OpResult::plain(Duration)),
        (Sub, Time, Duration) => return Some(OpResult::plain(Time)),
        (Add | Sub, Duration, Duration) => return Some(OpResult::plain(Duration)),
        (Mul, Duration, Amount) | (Mul, Amount, Duration) => {
            return Some(OpResult::plain(Duration))
        }
        (Div, Duration, Duration) => return Some(OpResult::plain(Amount)),
        (Div, Duration, Amount) => return Some(OpResult::plain(Duration)),
        _ => {}
    }

    // FHE: both sides ciphertext.
    if let (Ciphertext(a), Ciphertext(b)) = (lhs, rhs) {
        if a == b && matches!(op, Add | Sub | Mul) {
            return Some(OpResult::plain(Ciphertext(a.clone())));
        }
    }

    // FHE: mixed plaintext/ciphertext — auto-lift.
    if let (Ciphertext(a), b) = (lhs, rhs) {
        if **a == *b && matches!(op, Add | Sub | Mul) {
            return Some(OpResult::lift_right(Ciphertext(a.clone())));
        }
    }
    if let (a, Ciphertext(b)) = (lhs, rhs) {
        if *a == **b && matches!(op, Add | Sub | Mul) {
            return Some(OpResult::lift_left(Ciphertext(b.clone())));
        }
    }

    None
}

fn comparison(op: BinaryOp, lhs: &Ty, rhs: &Ty) -> Option<OpResult> {
    use BinaryOp::*;
    use Ty::*;

    let is_eq_op = matches!(op, Eq | NotEq);
    let is_ord_op = matches!(op, Lt | LtEq | Gt | GtEq);

    // Equality on equal plaintext types.
    if is_eq_op && lhs == rhs && lhs.is_eq_comparable() {
        return Some(OpResult::plain(Bool));
    }

    // Ordering on ordered plaintext types.
    if is_ord_op && lhs == rhs && lhs.is_ord_comparable() {
        return Some(OpResult::plain(Bool));
    }

    // Encrypted × encrypted.
    if let (Ciphertext(a), Ciphertext(b)) = (lhs, rhs) {
        if a == b {
            if is_eq_op && a.is_eq_comparable() {
                return Some(OpResult::plain(Ciphertext(Box::new(Bool))));
            }
            if is_ord_op && a.is_ord_comparable() {
                return Some(OpResult::plain(Ciphertext(Box::new(Bool))));
            }
        }
    }

    // Encrypted × plaintext — lift plaintext.
    if let (Ciphertext(a), b) = (lhs, rhs) {
        if **a == *b {
            if is_eq_op && a.is_eq_comparable() {
                return Some(OpResult::lift_right(Ciphertext(Box::new(Bool))));
            }
            if is_ord_op && a.is_ord_comparable() {
                return Some(OpResult::lift_right(Ciphertext(Box::new(Bool))));
            }
        }
    }
    if let (a, Ciphertext(b)) = (lhs, rhs) {
        if *a == **b {
            if is_eq_op && a.is_eq_comparable() {
                return Some(OpResult::lift_left(Ciphertext(Box::new(Bool))));
            }
            if is_ord_op && a.is_ord_comparable() {
                return Some(OpResult::lift_left(Ciphertext(Box::new(Bool))));
            }
        }
    }

    None
}

fn boolean(lhs: &Ty, rhs: &Ty) -> Option<OpResult> {
    use Ty::*;

    if let (Bool, Bool) = (lhs, rhs) {
        return Some(OpResult::plain(Bool));
    }
    if let (Ciphertext(a), Ciphertext(b)) = (lhs, rhs) {
        if matches!(**a, Bool) && matches!(**b, Bool) {
            return Some(OpResult::plain(Ciphertext(Box::new(Bool))));
        }
    }
    if let (Ciphertext(a), Bool) = (lhs, rhs) {
        if matches!(**a, Bool) {
            return Some(OpResult::lift_right(Ciphertext(Box::new(Bool))));
        }
    }
    if let (Bool, Ciphertext(b)) = (lhs, rhs) {
        if matches!(**b, Bool) {
            return Some(OpResult::lift_left(Ciphertext(Box::new(Bool))));
        }
    }
    None
}

pub fn dispatch_unary(op: UnaryOp, operand: &Ty) -> Option<Ty> {
    use Ty::*;

    if matches!(operand, Unknown) {
        return Some(Unknown);
    }

    match (op, operand) {
        (UnaryOp::Not, Bool) => Some(Bool),
        (UnaryOp::Not, Ciphertext(inner)) if matches!(**inner, Bool) => {
            Some(Ciphertext(Box::new(Bool)))
        }
        (UnaryOp::Neg, Amount) => Some(Amount),
        (UnaryOp::Neg, Duration) => Some(Duration),
        (UnaryOp::BitNot, Bytes) => Some(Bytes),
        (UnaryOp::BitNot, Amount) => Some(Amount),
        _ => None,
    }
}

pub fn binop_name(op: BinaryOp) -> &'static str {
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
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::Concat => "++",
        BinaryOp::In => "in",
    }
}

pub fn unop_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "!",
        UnaryOp::Neg => "-",
        UnaryOp::BitNot => "~",
    }
}
