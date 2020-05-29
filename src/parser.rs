use std::mem;

use crate::{
    interpreter::Interpreter,
    lexer::{Lexer, Token, TokenKind},
    symbol_table::SymbolTable,
};

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl(String, Box<Expr>),
    FnDecl(String, Vec<String>, Box<Expr>),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Binary(Box<Expr>, TokenKind, Box<Expr>),
    Unary(TokenKind, Box<Expr>),
    Unit(Box<Expr>, TokenKind),
    Var(String),
    Group(Box<Expr>),
    FnCall(String, Vec<Expr>),
    Literal(String),
}

#[derive(Debug, Clone)]
pub enum Unit {
    Radians,
    Degrees,
}

pub struct ParserContext {
    //angle_unit: Unit,
    tokens: Vec<Token>,
    pos: usize,
    symbol_table: SymbolTable,
}

impl TokenKind {
    pub fn is_unit(&self) -> bool {
        match self {
            TokenKind::Deg | TokenKind::Rad => true,
            _ => false,
        }
    }

    pub fn compare(&self, second_token: &TokenKind) -> bool {
        mem::discriminant(self) == mem::discriminant(second_token)
    }
}

impl ParserContext {
    pub fn new() -> Self {
        ParserContext {
            tokens: Vec::new(),
            pos: 0,
            symbol_table: SymbolTable::new(),
        }
    }
}

pub fn parse(context: &mut ParserContext, input: &str, angle_unit: Unit) -> Result<f64, String> {
    context.tokens = Lexer::lex(input);

    let mut statements: Vec<Stmt> = Vec::new();
    while !is_at_end(context) {
        statements.push(parse_stmt(context)?);
    }

    let mut interpreter = Interpreter::new(angle_unit, &mut context.symbol_table);
    Ok(interpreter.interpret(statements).unwrap())
}

fn parse_stmt(context: &mut ParserContext) -> Result<Stmt, String> {
    if match_token(context, TokenKind::Identifier) {
        return Ok(match peek_next(context).kind {
            TokenKind::Equals => parse_var_decl_stmt(context)?,
            TokenKind::OpenParenthesis => parse_identifier_stmt(context)?,
            _ => Stmt::Expr(Box::new(parse_expr(context)?)),
        });
    }

    Ok(Stmt::Expr(Box::new(parse_expr(context)?)))
}

fn parse_identifier_stmt(context: &mut ParserContext) -> Result<Stmt, String> {
    let began_at = context.pos;
    let primary = parse_primary(context)?; // Since function declarations and function calls look the same at first, simply parse a "function call", and re-use the data.

    // If `primary` is followed by an equal sign, it is a function declaration.
    if let TokenKind::Equals = peek(context).kind {
        advance(context);
        let expr = parse_expr(context)?;

        // Use the "function call" expression that was parsed, and put its values into a function declaration statement instead.
        if let Expr::FnCall(identifier, parameters) = primary {
            let mut parameter_identifiers = Vec::new();

            // All the "arguments" are expected to be parsed as variables,
            // since parameter definitions look the same as variable references.
            // Extract these.
            for parameter in parameters {
                if let Expr::Var(parameter_identifier) = parameter {
                    parameter_identifiers.push(parameter_identifier);
                }
            }

            let fn_decl = Stmt::FnDecl(identifier.clone(), parameter_identifiers, Box::new(expr));

            // Insert the function declaration into the symbol table during parsing
            // so that the parser can find out if particular functions exist.
            context
                .symbol_table
                .insert(&format!("{}()", identifier), fn_decl.clone());

            return Ok(fn_decl);
        }

        Err("Parsing error.".into())
    } else {
        // It is a function call, not a function declaration.
        // Redo the parsing for this specific part.
        context.pos = began_at;
        Ok(Stmt::Expr(Box::new(parse_expr(context)?)))
    }
}

fn parse_var_decl_stmt(context: &mut ParserContext) -> Result<Stmt, String> {
    let identifier = advance(context).clone();
    advance(context); // Equal sign
    let expr = parse_expr(context)?;

    Ok(Stmt::VarDecl(identifier.value, Box::new(expr)))
}

fn parse_expr(context: &mut ParserContext) -> Result<Expr, String> {
    Ok(parse_sum(context)?)
}

fn parse_sum(context: &mut ParserContext) -> Result<Expr, String> {
    let mut left = parse_factor(context)?;

    while match_token(context, TokenKind::Plus) || match_token(context, TokenKind::Minus) {
        let op = peek(context).kind.clone();
        advance(context);
        let right = parse_factor(context)?;

        left = Expr::Binary(Box::new(left), op, Box::new(right));
    }

    Ok(left)
}

fn parse_factor(context: &mut ParserContext) -> Result<Expr, String> {
    let mut left = parse_unary(context)?;

    while match_token(context, TokenKind::Star)
        || match_token(context, TokenKind::Slash)
        || match_token(context, TokenKind::Identifier)
    {
        let mut op = peek(context).kind.clone();

        // If the next token is an identifier, assume it's multiplication. Eg. 3y
        if let TokenKind::Identifier = op {
            op = TokenKind::Star;
        } else {
            advance(context);
        }

        let right = parse_unary(context)?;
        left = Expr::Binary(Box::new(left), op, Box::new(right));
    }

    Ok(left)
}

fn parse_unary(context: &mut ParserContext) -> Result<Expr, String> {
    if match_token(context, TokenKind::Minus) {
        let op = advance(context).kind.clone();
        let expr = Box::new(parse_unary(context)?);
        return Ok(Expr::Unary(op, expr));
    }

    Ok(parse_exponent(context)?)
}

fn parse_exponent(context: &mut ParserContext) -> Result<Expr, String> {
    let left = parse_primary(context)?;

    if match_token(context, TokenKind::Power) {
        let op = advance(context).kind.clone();
        let right = Box::new(parse_exponent(context)?);
        return Ok(Expr::Binary(Box::new(left), op, right));
    }

    Ok(left)
}

fn parse_primary(context: &mut ParserContext) -> Result<Expr, String> {
    let expr = match peek(context).kind {
        TokenKind::OpenParenthesis => parse_group(context)?,
        TokenKind::Pipe => parse_abs(context)?,
        TokenKind::Identifier => parse_identifier(context)?,
        _ => Expr::Literal(advance(context).value.clone()),
    };

    if !is_at_end(context) && peek(context).kind.is_unit() {
        Ok(Expr::Unit(Box::new(expr), advance(context).kind.clone()))
    } else {
        Ok(expr)
    }
}

fn parse_group(context: &mut ParserContext) -> Result<Expr, String> {
    advance(context);
    let group_expr = Expr::Group(Box::new(parse_expr(context)?));
    consume(context, TokenKind::ClosedParenthesis)?;

    Ok(group_expr)
}

fn parse_abs(context: &mut ParserContext) -> Result<Expr, String> {
    advance(context);
    let group_expr = Expr::Group(Box::new(parse_expr(context)?));
    consume(context, TokenKind::Pipe)?;

    Ok(Expr::FnCall(String::from("abs"), vec![group_expr]))
}

fn parse_identifier(context: &mut ParserContext) -> Result<Expr, String> {
    let identifier = advance(context).clone();

    // Eg. sqrt64
    if match_token(context, TokenKind::Literal) {
        // If there is a function with this name, parse it as a function, with the next token as the argument.
        if context.symbol_table.contains_func(&identifier.value) {
            let parameter = Expr::Literal(advance(context).value.clone());
            return Ok(Expr::FnCall(identifier.value, vec![parameter]));
        }
    }

    // Eg. sqrt(64)
    if match_token(context, TokenKind::OpenParenthesis) {
        advance(context);

        let mut parameters = Vec::new();
        parameters.push(parse_expr(context)?);

        while match_token(context, TokenKind::Comma) {
            advance(context);
            parameters.push(parse_expr(context)?);
        }

        consume(context, TokenKind::ClosedParenthesis)?;

        return Ok(Expr::FnCall(identifier.value, parameters));
    }

    // Eg. x
    Ok(Expr::Var(identifier.value))
}

fn peek<'a>(context: &'a mut ParserContext) -> &'a Token {
    &context.tokens[context.pos]
}

fn peek_next<'a>(context: &'a mut ParserContext) -> &'a Token {
    &context.tokens[context.pos + 1]
}

fn previous<'a>(context: &'a mut ParserContext) -> &'a Token {
    &context.tokens[context.pos - 1]
}

fn match_token(context: &mut ParserContext, kind: TokenKind) -> bool {
    if is_at_end(context) {
        return false;
    }

    peek(context).kind.compare(&kind)
}

fn advance<'a>(context: &'a mut ParserContext) -> &'a Token {
    context.pos += 1;
    previous(context)
}

fn consume<'a>(context: &'a mut ParserContext, kind: TokenKind) -> Result<&'a Token, String> {
    if match_token(context, kind) {
        return Ok(advance(context));
    }

    Err("Unexpected token".into())
}

fn is_at_end(context: &mut ParserContext) -> bool {
    context.pos >= context.tokens.len() || peek(context).kind.compare(&TokenKind::EOF)
}