// tarraq - the reflective machine of Optic.
//
// One medium: S-expressions. Code and data are the same text.
// This host evaluator is the bootstrap: a small Lisp with a
// fuel meter on every step, environments as first-class
// values, and no ambient authority - a program here can only
// compute. The seed evaluator (seed.tq) is written in this
// language and carried as a document; the closing replaces
// this bootstrap with the system's own text, layer by layer.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

// ------------------------------------------------ values

#[derive(Clone)]
enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Sym(String),
    Str(String),
    Pair(Rc<RefCell<(Value, Value)>>),
    Closure(Rc<Closure>),
    Prim(String),
}

struct Closure {
    fixed: Vec<String>,
    rest: Option<String>,
    body: Vec<Value>,
    env: Env,
}

fn cons(a: Value, b: Value) -> Value {
    Value::Pair(Rc::new(RefCell::new((a, b))))
}

fn car(v: &Value) -> Result<Value, Error> {
    match v {
        Value::Pair(p) => Ok(p.borrow().0.clone()),
        _ => Err(Error::Type(format!("car of {}", show(v)))),
    }
}

fn cdr(v: &Value) -> Result<Value, Error> {
    match v {
        Value::Pair(p) => Ok(p.borrow().1.clone()),
        _ => Err(Error::Type(format!("cdr of {}", show(v)))),
    }
}

fn list_of(items: Vec<Value>) -> Value {
    let mut out = Value::Nil;
    for it in items.into_iter().rev() {
        out = cons(it, out);
    }
    out
}

fn vec_of(v: &Value) -> Result<Vec<Value>, Error> {
    let mut out = Vec::new();
    let mut cur = v.clone();
    loop {
        match cur {
            Value::Nil => return Ok(out),
            Value::Pair(p) => {
                let (a, d) = p.borrow().clone();
                out.push(a);
                cur = d;
            }
            _ => return Err(Error::Type(format!("not a proper list: {}", show(v)))),
        }
    }
}

fn is_truthy(v: &Value) -> bool {
    !matches!(v, Value::Bool(false))
}

fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Sym(x), Value::Sym(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Pair(x), Value::Pair(y)) => Rc::ptr_eq(x, y),
        (Value::Prim(x), Value::Prim(y)) => x == y,
        _ => false,
    }
}

fn value_equal(a: &Value, b: &Value) -> bool {
    if value_eq(a, b) {
        return true;
    }
    match (a, b) {
        (Value::Pair(x), Value::Pair(y)) => {
            let (xa, xd) = x.borrow().clone();
            let (ya, yd) = y.borrow().clone();
            value_equal(&xa, &ya) && value_equal(&xd, &yd)
        }
        _ => false,
    }
}

fn show(v: &Value) -> String {
    match v {
        Value::Nil => "()".to_string(),
        Value::Bool(true) => "#t".to_string(),
        Value::Bool(false) => "#f".to_string(),
        Value::Int(n) => n.to_string(),
        Value::Sym(s) => s.clone(),
        Value::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Pair(_) => {
            let mut parts = Vec::new();
            let mut cur = v.clone();
            loop {
                match cur {
                    Value::Pair(p) => {
                        let (a, d) = p.borrow().clone();
                        parts.push(show(&a));
                        cur = d;
                    }
                    Value::Nil => return format!("({})", parts.join(" ")),
                    ref other => {
                        return format!("({} . {})", parts.join(" "), show(other));
                    }
                }
            }
        }
        Value::Closure(_) => "<procedure>".to_string(),
        Value::Prim(n) => format!("<primitive {}>", n),
    }
}

// ------------------------------------------------ errors

#[derive(Debug)]
enum Error {
    Parse(String),
    Io(String),
    Unbound(String),
    Type(String),
    Arity(String),
    Failed(String),
    NotProcedure(String),
    Starved { spent: u64 },
    Depth,
    Cycle(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Parse(m) => write!(f, "parse error: {}", m),
            Error::Io(m) => write!(f, "io error: {}", m),
            Error::Unbound(n) => write!(f, "unbound variable: {}", n),
            Error::Type(m) => write!(f, "type error: {}", m),
            Error::Arity(m) => write!(f, "arity error: {}", m),
            Error::Failed(m) => write!(f, "failed: {}", m),
            Error::NotProcedure(m) => write!(f, "not a procedure: {}", m),
            Error::Starved { spent } => write!(f, "starved: fuel exhausted after {} steps", spent),
            Error::Depth => write!(f, "depth exhausted: evaluation nested past the host limit"),
            Error::Cycle(m) => write!(f, "cycle: {}", m),
        }
    }
}

// ------------------------------------------------ reader

struct Reader {
    chars: Vec<char>,
    pos: usize,
}

impl Reader {
    fn new(text: &str) -> Reader {
        Reader { chars: text.chars().collect(), pos: 0 }
    }

    fn skip_layout(&mut self) {
        loop {
            while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
                self.pos += 1;
            }
            if self.pos < self.chars.len() && self.chars[self.pos] == ';' {
                while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                    self.pos += 1;
                }
            } else {
                return;
            }
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_layout();
        self.chars.get(self.pos).copied()
    }

    fn read_expr(&mut self) -> Result<Value, Error> {
        let c = self.peek().ok_or_else(|| Error::Parse("unexpected end of input".into()))?;
        match c {
            '(' => {
                self.pos += 1;
                self.read_list()
            }
            ')' => Err(Error::Parse("unexpected )".into())),
            '\'' => {
                self.pos += 1;
                let inner = self.read_expr()?;
                Ok(list_of(vec![Value::Sym("quote".into()), inner]))
            }
            '"' => {
                self.pos += 1;
                let mut s = String::new();
                loop {
                    let ch = self.chars.get(self.pos).copied()
                        .ok_or_else(|| Error::Parse("unterminated string".into()))?;
                    self.pos += 1;
                    match ch {
                        '"' => break,
                        '\\' => {
                            let esc = self.chars.get(self.pos).copied()
                                .ok_or_else(|| Error::Parse("unterminated escape".into()))?;
                            self.pos += 1;
                            s.push(match esc {
                                'n' => '\n',
                                't' => '\t',
                                other => other,
                            });
                        }
                        other => s.push(other),
                    }
                }
                Ok(Value::Str(s))
            }
            _ => {
                let start = self.pos;
                while self.pos < self.chars.len() {
                    let ch = self.chars[self.pos];
                    if ch.is_whitespace() || ch == '(' || ch == ')' || ch == ';' || ch == '\'' {
                        break;
                    }
                    self.pos += 1;
                }
                let tok: String = self.chars[start..self.pos].iter().collect();
                Ok(atom(&tok))
            }
        }
    }

    fn read_list(&mut self) -> Result<Value, Error> {
        let mut items = Vec::new();
        loop {
            let c = self.peek().ok_or_else(|| Error::Parse("unterminated list".into()))?;
            if c == ')' {
                self.pos += 1;
                return Ok(list_of(items));
            }
            if c == '.' {
                // a dotted tail only when '.' is a whole token:
                // the character after it is layout or the close
                let after = self.chars.get(self.pos + 1).copied();
                if matches!(after, Some(ch) if ch.is_whitespace() || ch == ')') {
                    self.pos += 1;
                    let tail = self.read_expr()?;
                    let close = self.peek()
                        .ok_or_else(|| Error::Parse("unterminated dotted list".into()))?;
                    if close != ')' {
                        return Err(Error::Parse("expected ) after dotted tail".into()));
                    }
                    self.pos += 1;
                    let mut out = tail;
                    for it in items.into_iter().rev() {
                        out = cons(it, out);
                    }
                    return Ok(out);
                }
            }
            items.push(self.read_expr()?);
        }
    }
}

fn atom(tok: &str) -> Value {
    match tok {
        "#t" => return Value::Bool(true),
        "#f" => return Value::Bool(false),
        _ => {}
    }
    if let Ok(n) = tok.parse::<i64>() {
        if tok.chars().next().map(|c| c.is_ascii_digit() || c == '-' || c == '+').unwrap_or(false)
            && tok.len() > 1 || tok.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        {
            return Value::Int(n);
        }
        if tok.len() == 1 && tok.chars().next().unwrap().is_ascii_digit() {
            return Value::Int(n);
        }
    }
    Value::Sym(tok.to_string())
}

fn parse_all(text: &str) -> Result<Vec<Value>, Error> {
    let mut r = Reader::new(text);
    let mut out = Vec::new();
    while r.peek().is_some() {
        out.push(r.read_expr()?);
    }
    Ok(out)
}

fn parse_one(text: &str) -> Result<Value, Error> {
    let forms = parse_all(text)?;
    match forms.len() {
        1 => Ok(forms.into_iter().next().unwrap()),
        0 => Err(Error::Parse("empty input".into())),
        n => Err(Error::Parse(format!("expected one expression, got {}", n))),
    }
}

// ------------------------------------------------ environments

type Env = Rc<RefCell<Frame>>;

enum FrameKind {
    Vars(Vec<(String, Value)>),
    Board(Rc<RefCell<Board>>),
}

struct Frame {
    kind: FrameKind,
    parent: Option<Env>,
}

fn new_frame(vars: Vec<(String, Value)>, parent: Option<Env>) -> Env {
    Rc::new(RefCell::new(Frame { kind: FrameKind::Vars(vars), parent }))
}

fn lookup(env: &Env, name: &str) -> Result<Value, Error> {
    enum Step {
        Found(Value),
        Next(Option<Env>),
        Board(Rc<RefCell<Board>>, Option<Env>),
    }
    let mut cur: Option<Env> = Some(env.clone());
    while let Some(e) = cur {
        let step = {
            let frame = e.borrow();
            match &frame.kind {
                FrameKind::Vars(vars) => match vars.iter().find(|(n, _)| n == name) {
                    Some((_, v)) => Step::Found(v.clone()),
                    None => Step::Next(frame.parent.clone()),
                },
                FrameKind::Board(b) => Step::Board(b.clone(), frame.parent.clone()),
            }
        };
        match step {
            Step::Found(v) => return Ok(v),
            Step::Next(parent) => cur = parent,
            Step::Board(b, parent) => {
                if let Some(v) = board_lookup(&b, name)? {
                    return Ok(v);
                }
                cur = parent;
            }
        }
    }
    Err(Error::Unbound(name.to_string()))
}

fn define(env: &Env, name: String, val: Value) {
    let mut frame = env.borrow_mut();
    if let FrameKind::Vars(vars) = &mut frame.kind {
        if let Some(slot) = vars.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = val;
        } else {
            vars.push((name, val));
        }
    }
}

fn set_var(env: &Env, name: &str, val: Value) -> Result<(), Error> {
    let mut cur: Option<Env> = Some(env.clone());
    while let Some(e) = cur {
        let mut frame = e.borrow_mut();
        if let FrameKind::Vars(vars) = &mut frame.kind {
            if let Some(slot) = vars.iter_mut().find(|(n, _)| n == name) {
                slot.1 = val;
                return Ok(());
            }
        }
        cur = frame.parent.clone();
    }
    Err(Error::Unbound(name.to_string()))
}

// ------------------------------------------------ the fuel meter

struct Meter {
    fuel: Cell<u64>,
    spent: Cell<u64>,
    depth: Cell<usize>,
    max_depth: usize,
}

impl Meter {
    fn new(fuel: u64) -> Meter {
        Meter { fuel: Cell::new(fuel), spent: Cell::new(0), depth: Cell::new(0), max_depth: 50_000 }
    }

    fn spend(&self, n: u64) -> Result<(), Error> {
        if self.fuel.get() < n {
            return Err(Error::Starved { spent: self.spent.get() });
        }
        self.fuel.set(self.fuel.get() - n);
        self.spent.set(self.spent.get() + n);
        Ok(())
    }

    fn enter(&self) -> Result<DepthGuard, Error> {
        let d = self.depth.get();
        if d >= self.max_depth {
            return Err(Error::Depth);
        }
        self.depth.set(d + 1);
        Ok(DepthGuard(self))
    }
}

struct DepthGuard<'a>(&'a Meter);

impl<'a> Drop for DepthGuard<'a> {
    fn drop(&mut self) {
        self.0.depth.set(self.0.depth.get() - 1);
    }
}

// ------------------------------------------------ the evaluator

fn eval(exp: &Value, env: &Env, m: &Meter) -> Result<Value, Error> {
    let mut exp = exp.clone();
    let mut env = env.clone();
    loop {
        m.spend(1)?;
        match &exp {
            Value::Nil | Value::Bool(_) | Value::Int(_) | Value::Str(_) | Value::Closure(_)
            | Value::Prim(_) => return Ok(exp),
            Value::Sym(name) => return lookup(&env, name),
            Value::Pair(_) => {}
        }
        let _guard = m.enter()?;
        let head = car(&exp)?;
        if let Value::Sym(kw) = &head {
            match kw.as_str() {
                "quote" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.len() != 1 {
                        return Err(Error::Arity("quote takes one argument".into()));
                    }
                    return Ok(rest[0].clone());
                }
                "if" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.len() < 2 || rest.len() > 3 {
                        return Err(Error::Arity("if takes two or three arguments".into()));
                    }
                    let test = eval(&rest[0], &env, m)?;
                    exp = if is_truthy(&test) {
                        rest[1].clone()
                    } else if rest.len() == 3 {
                        rest[2].clone()
                    } else {
                        return Ok(Value::Nil);
                    };
                    continue;
                }
                "lambda" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.is_empty() {
                        return Err(Error::Arity("lambda needs a parameter list".into()));
                    }
                    let (fixed, rest_param) = parse_params(&rest[0])?;
                    return Ok(Value::Closure(Rc::new(Closure {
                        fixed,
                        rest: rest_param,
                        body: rest[1..].to_vec(),
                        env: env.clone(),
                    })));
                }
                "define" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.is_empty() {
                        return Err(Error::Arity("define needs a target".into()));
                    }
                    if let Value::Sym(name) = &rest[0] {
                        if rest.len() != 2 {
                            return Err(Error::Arity("define of a name takes one value".into()));
                        }
                        let val = eval(&rest[1], &env, m)?;
                        define(&env, name.clone(), val);
                        return Ok(Value::Sym(name.clone()));
                    }
                    if let Value::Pair(_) = &rest[0] {
                        let name = match car(&rest[0])? {
                            Value::Sym(n) => n,
                            _ => return Err(Error::Type("function name must be a symbol".into())),
                        };
                        let spec = cdr(&rest[0])?;
                        let (fixed, rest_param) = parse_params(&spec)?;
                        let closure = Value::Closure(Rc::new(Closure {
                            fixed,
                            rest: rest_param,
                            body: rest[1..].to_vec(),
                            env: env.clone(),
                        }));
                        define(&env, name.clone(), closure);
                        return Ok(Value::Sym(name));
                    }
                    return Err(Error::Type("define target must be a name or a signature".into()));
                }
                "set!" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.len() != 2 {
                        return Err(Error::Arity("set! takes a name and a value".into()));
                    }
                    let name = match &rest[0] {
                        Value::Sym(n) => n.clone(),
                        _ => return Err(Error::Type("set! target must be a symbol".into())),
                    };
                    let val = eval(&rest[1], &env, m)?;
                    set_var(&env, &name, val)?;
                    return Ok(Value::Sym(name));
                }
                "begin" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.is_empty() {
                        return Ok(Value::Nil);
                    }
                    for form in &rest[..rest.len() - 1] {
                        eval(form, &env, m)?;
                    }
                    exp = rest[rest.len() - 1].clone();
                    continue;
                }
                "let" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.len() < 2 {
                        return Err(Error::Arity("let needs bindings and a body".into()));
                    }
                    let bindings = vec_of(&rest[0])?;
                    let mut names = Vec::new();
                    let mut exprs = Vec::new();
                    for b in &bindings {
                        let pair = vec_of(b)?;
                        if pair.len() != 2 {
                            return Err(Error::Arity("each let binding is (name value)".into()));
                        }
                        let name = match &pair[0] {
                            Value::Sym(n) => n.clone(),
                            _ => return Err(Error::Type("let binding name must be a symbol".into())),
                        };
                        names.push(Value::Sym(name));
                        exprs.push(pair[1].clone());
                    }
                    let mut lam = vec![Value::Sym("lambda".into()), list_of(names)];
                    lam.extend(rest[1..].iter().cloned());
                    let mut form = vec![list_of(lam)];
                    form.extend(exprs);
                    exp = list_of(form);
                    continue;
                }
                "cond" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    exp = expand_cond(&rest)?;
                    continue;
                }
                "and" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.is_empty() {
                        return Ok(Value::Bool(true));
                    }
                    for form in &rest[..rest.len() - 1] {
                        if !is_truthy(&eval(form, &env, m)?) {
                            return Ok(Value::Bool(false));
                        }
                    }
                    exp = rest[rest.len() - 1].clone();
                    continue;
                }
                "or" => {
                    let rest = vec_of(&cdr(&exp)?)?;
                    if rest.is_empty() {
                        return Ok(Value::Bool(false));
                    }
                    for form in &rest[..rest.len() - 1] {
                        let v = eval(form, &env, m)?;
                        if is_truthy(&v) {
                            return Ok(v);
                        }
                    }
                    exp = rest[rest.len() - 1].clone();
                    continue;
                }
                _ => {}
            }
        }
        // application
        let parts = vec_of(&exp)?;
        if parts.is_empty() {
            return Err(Error::NotProcedure("cannot apply ()".into()));
        }
        let operator = eval(&parts[0], &env, m)?;
        let mut args = Vec::new();
        for arg in &parts[1..] {
            args.push(eval(arg, &env, m)?);
        }
        m.spend(1)?;
        match &operator {
            Value::Prim(_) => {
                return apply_value(&operator, args, m);
            }
            Value::Closure(c) => {
                let bound = bind_params(c, args, m)?;
                env = bound;
                let body = &c.body;
                if body.is_empty() {
                    return Ok(Value::Nil);
                }
                for form in &body[..body.len() - 1] {
                    eval(form, &env, m)?;
                }
                exp = body[body.len() - 1].clone();
                continue;
            }
            other => return Err(Error::NotProcedure(format!("cannot apply {}", show(other)))),
        }
    }
}

fn expand_cond(clauses: &[Value]) -> Result<Value, Error> {
    if clauses.is_empty() {
        return Ok(Value::Nil);
    }
    let first = vec_of(&clauses[0])?;
    if first.is_empty() {
        return Err(Error::Arity("empty cond clause".into()));
    }
    if let Value::Sym(kw) = &first[0] {
        if kw == "else" {
            if clauses.len() != 1 {
                return Err(Error::Arity("else must be the last cond clause".into()));
            }
            if first.len() == 1 {
                return Ok(Value::Bool(true));
            }
            let mut seq = vec![Value::Sym("begin".into())];
            seq.extend(first[1..].iter().cloned());
            return Ok(list_of(seq));
        }
    }
    let test = first[0].clone();
    let then: Value = if first.len() == 1 {
        test.clone()
    } else {
        let mut seq = vec![Value::Sym("begin".into())];
        seq.extend(first[1..].iter().cloned());
        list_of(seq)
    };
    let rest: Value = if clauses.len() == 1 {
        Value::Nil
    } else {
        let mut cond = vec![Value::Sym("cond".into())];
        cond.extend(clauses[1..].iter().cloned());
        list_of(cond)
    };
    Ok(list_of(vec![
        Value::Sym("if".into()),
        test,
        then,
        rest,
    ]))
}

fn parse_params(spec: &Value) -> Result<(Vec<String>, Option<String>), Error> {
    match parse_dotted_params(spec) {
        Some(p) => Ok(p),
        None => Err(Error::Type("parameter list must be symbols".into())),
    }
}

fn parse_dotted_params(spec: &Value) -> Option<(Vec<String>, Option<String>)> {
    let mut fixed = Vec::new();
    let mut cur = spec.clone();
    loop {
        match cur {
            Value::Nil => return Some((fixed, None)),
            Value::Sym(s) => return Some((fixed, Some(s))),
            Value::Pair(p) => {
                let (a, d) = p.borrow().clone();
                match a {
                    Value::Sym(s) => fixed.push(s),
                    _ => return None,
                }
                cur = d;
            }
            _ => return None,
        }
    }
}

fn bind_params(c: &Closure, args: Vec<Value>, m: &Meter) -> Result<Env, Error> {
    m.spend(1)?;
    if args.len() < c.fixed.len() {
        return Err(Error::Arity(format!(
            "too few arguments: {} given, {} needed",
            args.len(),
            c.fixed.len()
        )));
    }
    let mut vars: Vec<(String, Value)> = c.fixed.iter().cloned().zip(args.iter().take(c.fixed.len()).cloned()).collect();
    match &c.rest {
        Some(name) => {
            vars.push((name.clone(), list_of(args[c.fixed.len()..].to_vec())));
        }
        None => {
            if args.len() > c.fixed.len() {
                return Err(Error::Arity(format!(
                    "too many arguments: {} given, {} needed",
                    args.len(),
                    c.fixed.len()
                )));
            }
        }
    }
    Ok(new_frame(vars, Some(c.env.clone())))
}

fn apply_value(proc: &Value, args: Vec<Value>, m: &Meter) -> Result<Value, Error> {
    match proc {
        Value::Prim(name) => {
            if name == "apply" {
                // apply is a procedure like any other: applying
                // it means applying its first argument to the
                // list in its second, at any layer of the tower
                if args.len() != 2 {
                    return Err(Error::Arity("apply takes a procedure and an argument list".into()));
                }
                let inner = args[0].clone();
                let arglist = vec_of(&args[1])?;
                return apply_value(&inner, arglist, m);
            }
            apply_prim(name, args, m)
        }
        Value::Closure(c) => {
            let env = bind_params(c, args, m)?;
            let body = &c.body;
            if body.is_empty() {
                return Ok(Value::Nil);
            }
            for form in &body[..body.len() - 1] {
                eval(form, &env, m)?;
            }
            eval(&body[body.len() - 1], &env, m)
        }
        other => Err(Error::NotProcedure(format!("cannot apply {}", show(other)))),
    }
}

// ------------------------------------------------ primitives

fn prim_names() -> Vec<String> {
    let mut names: Vec<String> = [
        "+", "-", "*", "quotient", "remainder", "=", "<", ">", "<=", ">=",
        "cons", "list", "length", "append", "reverse",
        "list-ref", "null?", "pair?", "symbol?", "number?", "boolean?",
        "string?", "procedure?", "eq?", "equal?", "not", "map",
        "set-car!", "set-cdr!", "fail", "apply",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // the whole c<ad>+r family: car, cdr, caar, cadr, ..., cddddr
    for len in 1..=4usize {
        for bits in 0..(1usize << len) {
            let mut name = String::from("c");
            for i in (0..len).rev() {
                name.push(if (bits >> i) & 1 == 1 { 'a' } else { 'd' });
            }
            name.push('r');
            names.push(name);
        }
    }
    names
}

fn num(v: &Value) -> Result<i64, Error> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(Error::Type(format!("expected a number, got {}", show(v)))),
    }
}

fn apply_prim(name: &str, args: Vec<Value>, m: &Meter) -> Result<Value, Error> {
    // the c<ad>+r family: caar, cadr, caddr, cadddr, and friends
    if name.starts_with('c') && name.ends_with('r') && name.len() >= 3
        && name[1..name.len() - 1].chars().all(|c| c == 'a' || c == 'd')
    {
        if args.len() != 1 {
            return Err(Error::Arity(format!("{} takes one argument", name)));
        }
        let mut v = args[0].clone();
        for ch in name[1..name.len() - 1].chars().rev() {
            v = if ch == 'a' { car(&v)? } else { cdr(&v)? };
        }
        return Ok(v);
    }
    match name {
        "+" | "-" | "*" => {
            let mut nums = Vec::new();
            for a in &args {
                nums.push(num(a)?);
            }
            let r = match name {
                "+" => nums.iter().try_fold(0i64, |a, b| a.checked_add(*b))
                    .ok_or_else(|| Error::Failed("integer overflow".into()))?,
                "-" => {
                    if nums.is_empty() {
                        return Err(Error::Arity("- takes at least one argument".into()));
                    }
                    if nums.len() == 1 {
                        nums[0].checked_neg().ok_or_else(|| Error::Failed("integer overflow".into()))?
                    } else {
                        nums[1..].iter().try_fold(nums[0], |a, b| a.checked_sub(*b))
                            .ok_or_else(|| Error::Failed("integer overflow".into()))?
                    }
                }
                _ => nums.iter().try_fold(1i64, |a, b| a.checked_mul(*b))
                    .ok_or_else(|| Error::Failed("integer overflow".into()))?,
            };
            Ok(Value::Int(r))
        }
        "quotient" | "remainder" => {
            if args.len() != 2 {
                return Err(Error::Arity(format!("{} takes two arguments", name)));
            }
            let (a, b) = (num(&args[0])?, num(&args[1])?);
            if b == 0 {
                return Err(Error::Failed("division by zero".into()));
            }
            Ok(Value::Int(if name == "quotient" { a / b } else { a % b }))
        }
        "=" | "<" | ">" | "<=" | ">=" => {
            if args.len() != 2 {
                return Err(Error::Arity(format!("{} takes two arguments", name)));
            }
            let (a, b) = (num(&args[0])?, num(&args[1])?);
            Ok(Value::Bool(match name {
                "=" => a == b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                _ => a >= b,
            }))
        }
        "cons" => {
            if args.len() != 2 {
                return Err(Error::Arity("cons takes two arguments".into()));
            }
            Ok(cons(args[0].clone(), args[1].clone()))
        }
        "car" => {
            if args.len() != 1 {
                return Err(Error::Arity("car takes one argument".into()));
            }
            car(&args[0])
        }
        "cdr" => {
            if args.len() != 1 {
                return Err(Error::Arity("cdr takes one argument".into()));
            }
            cdr(&args[0])
        }
        "list" => Ok(list_of(args)),
        "length" => {
            if args.len() != 1 {
                return Err(Error::Arity("length takes one argument".into()));
            }
            Ok(Value::Int(vec_of(&args[0])?.len() as i64))
        }
        "append" => {
            if args.is_empty() {
                return Ok(Value::Nil);
            }
            let mut items = Vec::new();
            for a in &args[..args.len() - 1] {
                items.extend(vec_of(a)?);
            }
            let mut out = args[args.len() - 1].clone();
            for it in items.into_iter().rev() {
                out = cons(it, out);
            }
            Ok(out)
        }
        "reverse" => {
            if args.len() != 1 {
                return Err(Error::Arity("reverse takes one argument".into()));
            }
            let mut items = vec_of(&args[0])?;
            items.reverse();
            Ok(list_of(items))
        }
        "list-ref" => {
            if args.len() != 2 {
                return Err(Error::Arity("list-ref takes two arguments".into()));
            }
            let items = vec_of(&args[0])?;
            let n = num(&args[1])? as usize;
            items.get(n).cloned().ok_or_else(|| Error::Arity("list-ref out of range".into()))
        }
        "null?" => {
            if args.len() != 1 {
                return Err(Error::Arity("null? takes one argument".into()));
            }
            Ok(Value::Bool(matches!(args[0], Value::Nil)))
        }
        "pair?" => {
            if args.len() != 1 {
                return Err(Error::Arity("pair? takes one argument".into()));
            }
            Ok(Value::Bool(matches!(args[0], Value::Pair(_))))
        }
        "symbol?" => {
            if args.len() != 1 {
                return Err(Error::Arity("symbol? takes one argument".into()));
            }
            Ok(Value::Bool(matches!(args[0], Value::Sym(_))))
        }
        "number?" => {
            if args.len() != 1 {
                return Err(Error::Arity("number? takes one argument".into()));
            }
            Ok(Value::Bool(matches!(args[0], Value::Int(_))))
        }
        "boolean?" => {
            if args.len() != 1 {
                return Err(Error::Arity("boolean? takes one argument".into()));
            }
            Ok(Value::Bool(matches!(args[0], Value::Bool(_))))
        }
        "string?" => {
            if args.len() != 1 {
                return Err(Error::Arity("string? takes one argument".into()));
            }
            Ok(Value::Bool(matches!(args[0], Value::Str(_))))
        }
        "procedure?" => {
            if args.len() != 1 {
                return Err(Error::Arity("procedure? takes one argument".into()));
            }
            Ok(Value::Bool(matches!(args[0], Value::Closure(_) | Value::Prim(_))))
        }
        "eq?" => {
            if args.len() != 2 {
                return Err(Error::Arity("eq? takes two arguments".into()));
            }
            Ok(Value::Bool(value_eq(&args[0], &args[1])))
        }
        "equal?" => {
            if args.len() != 2 {
                return Err(Error::Arity("equal? takes two arguments".into()));
            }
            Ok(Value::Bool(value_equal(&args[0], &args[1])))
        }
        "not" => {
            if args.len() != 1 {
                return Err(Error::Arity("not takes one argument".into()));
            }
            Ok(Value::Bool(!is_truthy(&args[0])))
        }
        "map" => {
            if args.len() != 2 {
                return Err(Error::Arity("map takes a procedure and a list".into()));
            }
            let items = vec_of(&args[1])?;
            let mut out = Vec::new();
            for item in items {
                out.push(apply_value(&args[0], vec![item], m)?);
            }
            Ok(list_of(out))
        }
        "set-car!" => {
            if args.len() != 2 {
                return Err(Error::Arity("set-car! takes a pair and a value".into()));
            }
            match &args[0] {
                Value::Pair(p) => {
                    p.borrow_mut().0 = args[1].clone();
                    Ok(Value::Nil)
                }
                _ => Err(Error::Type("set-car! needs a pair".into())),
            }
        }
        "set-cdr!" => {
            if args.len() != 2 {
                return Err(Error::Arity("set-cdr! takes a pair and a value".into()));
            }
            match &args[0] {
                Value::Pair(p) => {
                    p.borrow_mut().1 = args[1].clone();
                    Ok(Value::Nil)
                }
                _ => Err(Error::Type("set-cdr! needs a pair".into())),
            }
        }
        "fail" => {
            if args.len() != 1 {
                return Err(Error::Arity("fail takes one argument".into()));
            }
            Err(Error::Failed(show(&args[0])))
        }
        _ => Err(Error::NotProcedure(format!("unknown primitive {}", name))),
    }
}

fn global_env() -> Env {
    let mut vars: Vec<(String, Value)> = prim_names()
        .iter()
        .map(|n| (n.to_string(), Value::Prim(n.to_string())))
        .collect();
    let alist: Vec<Value> = prim_names()
        .iter()
        .map(|n| cons(Value::Sym(n.to_string()), Value::Prim(n.to_string())))
        .collect();
    vars.push(("*primitives*".to_string(), list_of(alist)));
    new_frame(vars, None)
}

// ------------------------------------------------ the board

struct Board {
    cells: HashMap<String, Value>,
    values: HashMap<String, Value>,
    busy: Vec<String>,
    fuel: u64,
    self_env: Option<Env>,
}

fn board_cell_names(b: &Rc<RefCell<Board>>) -> Vec<String> {
    let mut names: Vec<String> = b.borrow().cells.keys().cloned().collect();
    names.sort();
    names
}

fn board_ensure(b: &Rc<RefCell<Board>>, name: &str) -> Result<Value, Error> {
    {
        let board = b.borrow();
        if let Some(v) = board.values.get(name) {
            return Ok(v.clone());
        }
        if board.busy.contains(&name.to_string()) {
            let mut chain = board.busy.clone();
            chain.push(name.to_string());
            return Err(Error::Cycle(chain.join(" -> ")));
        }
        if !board.cells.contains_key(name) {
            return Err(Error::Unbound(format!("no cell named {}", name)));
        }
    }
    let (expr, env, fuel) = {
        let mut board = b.borrow_mut();
        board.busy.push(name.to_string());
        (
            board.cells.get(name).unwrap().clone(),
            board.self_env.clone().ok_or_else(|| Error::Io("board has no environment".into()))?,
            board.fuel,
        )
    };
    let meter = Meter::new(fuel);
    let result = eval(&expr, &env, &meter);
    let mut board = b.borrow_mut();
    board.busy.retain(|n| n != name);
    match result {
        Ok(v) => {
            board.values.insert(name.to_string(), v.clone());
            Ok(v)
        }
        Err(e) => Err(e),
    }
}

fn board_lookup(b: &Rc<RefCell<Board>>, name: &str) -> Result<Option<Value>, Error> {
    let is_cell = b.borrow().cells.contains_key(name);
    if !is_cell {
        return Ok(None);
    }
    board_ensure(b, name).map(Some)
}

fn cell_expr_from_form(form: &Value) -> Value {
    // (define name expr) or (define (name args) body...) unwrap to
    // the cell's expression; anything else is the expression.
    if let Ok(head) = car(form) {
        if let Value::Sym(kw) = &head {
            if kw == "define" {
                if let Ok(rest) = vec_of(&cdr(form).unwrap_or(Value::Nil)) {
                    if rest.len() >= 2 {
                        match &rest[0] {
                            Value::Sym(_) => return rest[1].clone(),
                            Value::Pair(_) => {
                                if let Ok(spec) = cdr(&rest[0]) {
                                    if let Some((fixed, rest_param)) = parse_dotted_params(&spec) {
                                        let fixed_syms: Vec<Value> =
                                            fixed.iter().map(|p| Value::Sym(p.clone())).collect();
                                        let params_value = match &rest_param {
                                            Some(r) => {
                                                let mut tail = Value::Sym(r.clone());
                                                for s in fixed_syms.iter().rev() {
                                                    tail = cons(s.clone(), tail);
                                                }
                                                tail
                                            }
                                            None => list_of(fixed_syms),
                                        };
                                        let mut lam = vec![Value::Sym("lambda".into()), params_value];
                                        lam.extend(rest[1..].iter().cloned());
                                        return list_of(lam);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    form.clone()
}

fn load_board(dir: &str, fuel: u64) -> Result<(Rc<RefCell<Board>>, Env), Error> {
    let cells_dir = format!("{}/cells", dir);
    let mut cells: HashMap<String, Value> = HashMap::new();
    let entries = std::fs::read_dir(&cells_dir)
        .map_err(|e| Error::Io(format!("cannot read {}: {}", cells_dir, e)))?;
    let mut paths: Vec<(String, std::path::PathBuf)> = Vec::new();
    for ent in entries {
        let ent = ent.map_err(|e| Error::Io(e.to_string()))?;
        let path = ent.path();
        if path.extension().map(|x| x == "tq").unwrap_or(false) {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            paths.push((stem, path));
        }
    }
    paths.sort();
    for (stem, path) in paths {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| Error::Io(format!("cannot read {}: {}", path.display(), e)))?;
        let forms = parse_all(&text)?;
        if forms.len() != 1 {
            return Err(Error::Parse(format!(
                "cell {} must hold exactly one form, holds {}",
                stem,
                forms.len()
            )));
        }
        cells.insert(stem, cell_expr_from_form(&forms[0]));
    }
    let board = Rc::new(RefCell::new(Board {
        cells,
        values: HashMap::new(),
        busy: Vec::new(),
        fuel,
        self_env: None,
    }));
    let env = Rc::new(RefCell::new(Frame {
        kind: FrameKind::Board(board.clone()),
        parent: Some(global_env()),
    }));
    board.borrow_mut().self_env = Some(env.clone());
    Ok((board, env))
}

// ------------------------------------------------ the seed and the closing

fn load_seed(seed_text: &str, fuel: u64) -> Result<Env, Error> {
    let g = global_env();
    let meter = Meter::new(fuel);
    for form in parse_all(seed_text)? {
        eval(&form, &g, &meter)?;
    }
    let driver = parse_one("(define *guest-env* (setup-environment))")?;
    eval(&driver, &g, &meter)?;
    Ok(g)
}

fn replace_last(haystack: &str, needle: &str, replacement: &str) -> String {
    match haystack.rfind(needle) {
        Some(i) => {
            let mut out = haystack[..i].to_string();
            out.push_str(replacement);
            out.push_str(&haystack[i + needle.len()..]);
            out
        }
        None => haystack.to_string(),
    }
}

// Run a guest expression under the seed (times = 0), or under
// the seed's own interpreted text (times >= 1: the closing,
// that many production layers deep). Returns the value and the
// fuel the guest run itself spent.
fn seed_run(seed_text: &str, guest: &str, times: usize, fuel: u64) -> Result<(Value, u64), Error> {
    let g = load_seed(seed_text, fuel)?;
    let seed_begin = format!("(begin {})", seed_text);
    // the closing loads: each replaces the guest env's evaluator
    // with a copy produced by the previous one
    let mut load_form = format!("(ev (quote {}) *guest-env*)", seed_begin);
    for level in 0..times {
        if level > 0 {
            load_form = format!(
                "(ev (quote {}) *guest-env*)",
                replace_last(&load_form, "*guest-env*", "*the-env*")
            );
        }
        let meter = Meter::new(fuel);
        eval(&parse_one(&load_form)?, &g, &meter)?;
    }
    let meter = Meter::new(fuel);
    let value = if times == 0 {
        let run_form = format!("(ev (quote {}) *guest-env*)", guest);
        eval(&parse_one(&run_form)?, &g, &meter)?
    } else {
        // The closing run: the guest is handed to the layer's
        // evaluator as a procedure, applied through the host's
        // own ap, so the evaluator's body is interpreted by
        // the host while its logic does the evaluating.
        // Feeding an inner (ev ...) form to the evaluator as a
        // guest form instead would make the implementation its
        // own guest: it then evaluates its own body forms, and
        // its cond clause matches its own body forever.
        let grab = parse_one("(define *data-ev* (lookup-variable (quote ev) *guest-env*))")?;
        eval(&grab, &g, &meter)?;
        let run_form = format!("(ap *data-ev* (list (quote {}) *guest-env*))", guest);
        eval(&parse_one(&run_form)?, &g, &meter)?
    };
    Ok((value, meter.spent.get()))
}

// ------------------------------------------------ commands

fn read_file(path: &str) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|e| Error::Io(format!("cannot read {}: {}", path, e)))
}

fn print_result(value: &Value, meter: &Meter) {
    println!("{}", show(value));
    println!("fuel: {}", meter.spent.get());
}

fn cmd_eval(expr: &str, fuel: u64) -> Result<(), Error> {
    let env = global_env();
    let meter = Meter::new(fuel);
    let value = eval(&parse_one(expr)?, &env, &meter)?;
    print_result(&value, &meter);
    Ok(())
}

fn cmd_run(path: &str, expr: Option<&str>, fuel: u64) -> Result<(), Error> {
    let env = global_env();
    let meter = Meter::new(fuel);
    let mut last = Value::Nil;
    for form in parse_all(&read_file(path)?)? {
        last = eval(&form, &env, &meter)?;
    }
    if let Some(e) = expr {
        last = eval(&parse_one(e)?, &env, &meter)?;
    }
    print_result(&last, &meter);
    Ok(())
}

fn repl(env: &Env, fuel: u64) {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let meter = Meter::new(fuel);
        match parse_all(&line).and_then(|forms| {
            let mut last = Value::Nil;
            for form in &forms {
                last = eval(form, env, &meter)?;
            }
            Ok(last)
        }) {
            Ok(v) => {
                println!("{}", show(&v));
                println!("fuel: {}", meter.spent.get());
            }
            Err(e) => println!("error: {}", e),
        }
    }
}

fn cmd_board_view(dir: &str, fuel: u64) -> Result<(), Error> {
    let (board, _env) = load_board(dir, fuel)?;
    println!("board: {}", dir);
    for name in board_cell_names(&board) {
        match board_ensure(&board, &name) {
            Ok(v) => println!("- {} = {}", name, show(&v)),
            Err(Error::Starved { spent }) => println!("- {} = starved after {} steps", name, spent),
            Err(e) => println!("- {} = error: {}", name, e),
        }
    }
    Ok(())
}

fn cmd_board_eval(dir: &str, expr: &str, fuel: u64) -> Result<(), Error> {
    let (_board, env) = load_board(dir, fuel)?;
    let meter = Meter::new(fuel);
    let value = eval(&parse_one(expr)?, &env, &meter)?;
    print_result(&value, &meter);
    Ok(())
}

fn cmd_board_set(dir: &str, name: &str) -> Result<(), Error> {
    use std::io::Read;
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| Error::Io(e.to_string()))?;
    let forms = parse_all(&text)?;
    if forms.len() != 1 {
        return Err(Error::Parse(format!(
            "a cell holds exactly one form, got {}",
            forms.len()
        )));
    }
    let cells_dir = format!("{}/cells", dir);
    std::fs::create_dir_all(&cells_dir).map_err(|e| Error::Io(e.to_string()))?;
    std::fs::write(format!("{}/{}.tq", cells_dir, name), text)
        .map_err(|e| Error::Io(e.to_string()))?;
    println!("cell {} set", name);
    Ok(())
}

fn cmd_board_repl(dir: &str, fuel: u64) -> Result<(), Error> {
    let (_board, env) = load_board(dir, fuel)?;
    let session = new_frame(Vec::new(), Some(env));
    repl(&session, fuel);
    Ok(())
}

fn cmd_seed_eval(seed_path: &str, expr: &str, times: usize, fuel: u64) -> Result<(), Error> {
    let seed_text = read_file(seed_path)?;
    let (value, spent) = seed_run(&seed_text, expr, times, fuel)?;
    println!("{}", show(&value));
    println!("fuel: {}", spent);
    Ok(())
}

fn usage() -> ! {
    eprintln!("tarraq - the reflective machine");
    eprintln!("usage:");
    eprintln!("  tarraq eval [--fuel N] <expr>");
    eprintln!("  tarraq run [--fuel N] <file> [expr]");
    eprintln!("  tarraq repl [--fuel N]");
    eprintln!("  tarraq board view <dir> [--fuel N]");
    eprintln!("  tarraq board eval <dir> <expr> [--fuel N]");
    eprintln!("  tarraq board set <dir> <name>     (definition on stdin)");
    eprintln!("  tarraq board repl <dir> [--fuel N]");
    eprintln!("  tarraq seed-eval [--fuel N] <seed-file> <expr>");
    eprintln!("  tarraq close-eval [--fuel N] [--times K] <seed-file> <expr>");
    std::process::exit(2);
}

fn take_fuel(args: &mut Vec<String>) -> u64 {
    let mut fuel = 1_000_000u64;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--fuel" && i + 1 < args.len() {
            fuel = args[i + 1].parse().unwrap_or(fuel);
            args.drain(i..=i + 1);
        } else {
            i += 1;
        }
    }
    fuel
}

fn take_times(args: &mut Vec<String>) -> usize {
    let mut times = 1usize;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--times" && i + 1 < args.len() {
            times = args[i + 1].parse().unwrap_or(times);
            args.drain(i..=i + 1);
        } else {
            i += 1;
        }
    }
    times
}

fn real_main() -> i32 {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    let cmd = args.remove(0);
    let fuel = take_fuel(&mut args);
    let result: Result<(), Error> = match cmd.as_str() {
        "eval" => {
            if args.len() != 1 {
                usage()
            }
            cmd_eval(&args[0], fuel)
        }
        "run" => {
            if args.len() < 1 || args.len() > 2 {
                usage()
            }
            cmd_run(&args[0], args.get(1).map(|s| s.as_str()), fuel)
        }
        "repl" => {
            repl(&global_env(), fuel);
            Ok(())
        }
        "board" => {
            if args.is_empty() {
                usage()
            }
            let sub = args.remove(0);
            match sub.as_str() {
                "view" => {
                    if args.len() != 1 {
                        usage()
                    }
                    cmd_board_view(&args[0], fuel)
                }
                "eval" => {
                    if args.len() != 2 {
                        usage()
                    }
                    cmd_board_eval(&args[0], &args[1], fuel)
                }
                "set" => {
                    if args.len() != 2 {
                        usage()
                    }
                    cmd_board_set(&args[0], &args[1])
                }
                "repl" => {
                    if args.len() != 1 {
                        usage()
                    }
                    cmd_board_repl(&args[0], fuel)
                }
                _ => usage(),
            }
        }
        "seed-eval" => {
            if args.len() != 2 {
                usage()
            }
            cmd_seed_eval(&args[0], &args[1], 0, fuel)
        }
        "close-eval" => {
            let times = take_times(&mut args);
            if args.len() != 2 {
                usage()
            }
            cmd_seed_eval(&args[0], &args[1], times, fuel)
        }
        _ => usage(),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {}", e);
            1
        }
    }
}

fn main() {
    // deep evaluation wants a deep stack; the fuel and depth
    // meters are the real limits, the stack just has to fit them
    let child = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(real_main)
        .expect("thread spawn");
    std::process::exit(child.join().unwrap_or(1));
}

// ------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, fuel: u64) -> Result<Value, Error> {
        let env = global_env();
        let meter = Meter::new(fuel);
        let mut last = Value::Nil;
        for form in parse_all(text)? {
            last = eval(&form, &env, &meter)?;
        }
        Ok(last)
    }

    fn run_int(text: &str) -> i64 {
        match run(text, 1_000_000).expect("eval failed") {
            Value::Int(n) => n,
            other => panic!("expected an integer, got {}", show(&other)),
        }
    }

    #[test]
    fn reader_roundtrips() {
        let v = parse_one("(1 (2 . 3) 'four \"five\" #t)").unwrap();
        assert_eq!(show(&v), "(1 (2 . 3) (quote four) \"five\" #t)");
        let dotted = parse_one("(a b . c)").unwrap();
        assert_eq!(show(&dotted), "(a b . c)");
        let forms = parse_all("; a comment\n(+ 1 2) ; another\n(* 3 4)").unwrap();
        assert_eq!(forms.len(), 2);
    }

    #[test]
    fn arithmetic_and_forms() {
        assert_eq!(run_int("(+ 1 2 3)"), 6);
        assert_eq!(run_int("(* (+ 1 2) (- 10 4))"), 18);
        assert_eq!(run_int("(quotient 17 5)"), 3);
        assert_eq!(run_int("(remainder 17 5)"), 2);
        assert_eq!(run_int("(let ((a 3) (b 4)) (+ a b))"), 7);
        assert_eq!(run_int("(cond ((= 1 2) 10) ((= 1 1) 20) (else 30))"), 20);
        assert_eq!(run_int("(caddr '(10 20 30 40))"), 30);
        assert_eq!(run_int("(begin (define x 5) (set! x (+ x 1)) x)"), 6);
        assert_eq!(show(&run("(and #t #f)", 1000).unwrap()), "#f");
        assert_eq!(show(&run("(or #f 7)", 1000).unwrap()), "7");
    }

    #[test]
    fn recursion_and_closures() {
        let fact = "(begin
            (define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))
            (fact 10))";
        assert_eq!(run_int(fact), 3628800);
        let counter = "(begin
            (define (make-counter)
              (define n 0)
              (lambda () (set! n (+ n 1)) n))
            (define c (make-counter))
            (c) (c) (c))";
        assert_eq!(run_int(counter), 3);
        let variadic = "(begin (define (f a . rest) (+ a (length rest))) (f 10 1 2 3))";
        assert_eq!(run_int(variadic), 13);
    }

    #[test]
    fn y_combinator_factorial() {
        // Turing completeness, shown: recursion from self
        // application alone, no define of the recursive name
        let y = "(begin
            (define Y
              (lambda (f)
                ((lambda (x) (f (lambda (v) ((x x) v))))
                 (lambda (x) (f (lambda (v) ((x x) v)))))))
            ((Y (lambda (fact)
                  (lambda (n)
                    (if (= n 0) 1 (* n (fact (- n 1)))))))
             5))";
        assert_eq!(run_int(y), 120);
    }

    #[test]
    fn omega_starves() {
        let omega = "((lambda (x) (x x)) (lambda (x) (x x)))";
        match run(omega, 10_000) {
            Err(Error::Starved { .. }) => {}
            other => panic!("omega should starve, got {:?}", other.map(|v| show(&v))),
        }
    }

    fn seed_text() -> String {
        let path = format!("{}/seed.tq", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(path).expect("seed.tq must sit beside Cargo.toml")
    }

    const GUEST_FACT: &str =
        "(begin (define (fact n) (if (= n 0) 1 (* n (fact (- n 1))))) (fact 5))";

    #[test]
    fn seed_evaluates() {
        let (value, _spent) = seed_run(&seed_text(), GUEST_FACT, 0, 1_000_000).expect("seed run");
        assert_eq!(show(&value), "120");
    }

    #[test]
    fn seed_evaluates_itself_in_the_guest() {
        // the guest names its own environment and hands a
        // program to the evaluator it carries
        let (value, _) = seed_run(&seed_text(), "(ev (quote (+ 20 22)) *the-env*)", 0, 1_000_000)
            .expect("guest ev");
        assert_eq!(show(&value), "42");
    }

    #[test]
    fn the_closing_two_layers() {
        let (v0, f0) = seed_run(&seed_text(), GUEST_FACT, 0, 20_000_000).expect("seed layer");
        assert_eq!(show(&v0), "120");
        let (v1, f1) = seed_run(&seed_text(), GUEST_FACT, 1, 40_000_000).expect("closing x1");
        assert_eq!(show(&v1), "120");
        let (v2, f2) = seed_run(&seed_text(), GUEST_FACT, 2, 200_000_000).expect("closing x2");
        assert_eq!(show(&v2), "120");
        // the fixed point: the interpreted evaluator is
        // structurally identical to the one that produced it,
        // so the second layer costs exactly the first layer's
        // fuel - while both cost far more than the host seed
        assert!(f1 > f0, "interpreted layer must cost more than the seed: {} then {}", f0, f1);
        assert_eq!(f1, f2, "the closing is a fixed point: {} then {}", f1, f2);
    }

    #[test]
    fn seed_omega_starves() {
        match seed_run(&seed_text(), "((lambda (x) (x x)) (lambda (x) (x x)))", 0, 100_000) {
            Err(Error::Starved { .. }) => {}
            other => panic!("seed omega should starve, got {:?}", other.map(|(v, _)| show(&v))),
        }
    }

    fn write_board(cells: &[(&str, &str)], fuel: u64) -> (String, Rc<RefCell<Board>>, Env) {
        let dir = std::env::temp_dir().join(format!("tarraq-board-test-{}", std::process::id()));
        let cells_dir = dir.join("cells");
        std::fs::create_dir_all(&cells_dir).unwrap();
        for (name, text) in cells {
            std::fs::write(cells_dir.join(format!("{}.tq", name)), text).unwrap();
        }
        let dir_str = dir.to_string_lossy().to_string();
        let (board, env) = load_board(&dir_str, fuel).expect("board loads");
        (dir_str, board, env)
    }

    #[test]
    fn board_mutual_recursion() {
        let (_dir, board, _env) = write_board(
            &[
                ("even", "(define (even n) (if (= n 0) #t (odd (- n 1))))"),
                ("odd", "(define (odd n) (if (= n 0) #f (even (- n 1))))"),
                ("answer", "(if (even 10) 42 0)"),
            ],
            1_000_000,
        );
        let v = board_ensure(&board, "answer").expect("answer cell");
        assert_eq!(show(&v), "42");
        assert!(board_cell_names(&board).contains(&"even".to_string()));
    }

    #[test]
    fn board_cycle_is_named() {
        let (_dir, board, _env) = write_board(
            &[("a", "(+ b 1)"), ("b", "(+ a 1)")],
            1_000_000,
        );
        match board_ensure(&board, "a") {
            Err(Error::Cycle(chain)) => {
                assert!(chain.contains('a') && chain.contains('b'), "chain: {}", chain);
            }
            other => panic!("expected a cycle error, got {:?}", other.map(|v| show(&v))),
        }
    }

    #[test]
    fn board_starved_cell_leaves_others() {
        let (_dir, board, _env) = write_board(
            &[
                ("loop", "((lambda (x) (x x)) (lambda (x) (x x)))"),
                ("fine", "(+ 40 2)"),
            ],
            2_000,
        );
        match board_ensure(&board, "loop") {
            Err(Error::Starved { .. }) => {}
            other => panic!("loop cell should starve, got {:?}", other.map(|v| show(&v))),
        }
        let v = board_ensure(&board, "fine").expect("fine cell");
        assert_eq!(show(&v), "42");
    }
}
