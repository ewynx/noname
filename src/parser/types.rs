use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{
    cli::packages::UserRepo,
    constants::{Field, Span},
    error::{ErrorKind, Result},
    lexer::{Keyword, Token, TokenKind, Tokens},
    syntax::is_type,
};

use super::{Expr, ExprKind, ParserCtx};

pub fn parse_type_declaration(
    ctx: &mut ParserCtx,
    tokens: &mut Tokens,
    ident: Ident,
) -> Result<Expr> {
    if !is_type(&ident.value) {
        panic!("this looks like a type declaration but not on a type (types start with an uppercase) (TODO: better error)");
    }

    // Thing { x: 1, y: 2 }
    //       ^
    tokens.bump(ctx);

    let mut fields = vec![];

    // Thing { x: 1, y: 2 }
    //         ^^^^^^^^^^^^
    loop {
        // Thing { x: 1, y: 2 }
        //                    ^
        if let Some(Token {
            kind: TokenKind::RightCurlyBracket,
            ..
        }) = tokens.peek()
        {
            tokens.bump(ctx);
            break;
        };

        // Thing { x: 1, y: 2 }
        //         ^
        let field_name = Ident::parse(ctx, tokens)?;

        // Thing { x: 1, y: 2 }
        //          ^
        tokens.bump_expected(ctx, TokenKind::Colon)?;

        // Thing { x: 1, y: 2 }
        //            ^
        let field_value = Expr::parse(ctx, tokens)?;
        fields.push((field_name, field_value));

        // Thing { x: 1, y: 2 }
        //             ^      ^
        match tokens.bump_err(ctx, ErrorKind::InvalidEndOfLine)? {
            Token {
                kind: TokenKind::Comma,
                ..
            } => (),
            Token {
                kind: TokenKind::RightCurlyBracket,
                ..
            } => break,
            _ => return Err(ctx.error(ErrorKind::InvalidEndOfLine, ctx.last_span())),
        };
    }

    let span = ident.span.merge_with(ctx.last_span());

    Ok(Expr::new(
        ctx,
        ExprKind::CustomTypeDeclaration {
            struct_name: ident,
            fields,
        },
        span,
    ))
}

pub fn parse_fn_call_args(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<(Vec<Expr>, Span)> {
    let start = tokens.bump(ctx).expect("parser error: parse_fn_call_args"); // (
    let mut span = start.span;

    let mut args = vec![];
    loop {
        let pp = tokens.peek();

        match pp {
            Some(x) => match x.kind {
                // ,
                TokenKind::Comma => {
                    tokens.bump(ctx);
                }

                // )
                TokenKind::RightParen => {
                    let end = tokens.bump(ctx).unwrap();
                    span = span.merge_with(end.span);
                    break;
                }

                // an argument (as expression)
                _ => {
                    let arg = Expr::parse(ctx, tokens)?;

                    args.push(arg);
                }
            },

            None => {
                return Err(ctx.error(
                    ErrorKind::InvalidFnCall("unexpected end of function call"),
                    ctx.last_span(),
                ))
            }
        }
    }

    Ok((args, span))
}

//~
//~ ## Type
//~
//~ Backus–Naur Form (BNF) grammar:
//~
//~ type ::=
//~     | /[A-Z] (A-Za-z0-9)*/
//~     | "[" type ";" numeric "]"
//~
//~ numeric ::= /[0-9]+/
//~

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ty {
    pub kind: TyKind,
    pub span: Span,
}

pub enum TypeModule {
    Alias(Ident),
    Absolute(UsePath),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TyKind {
    /// The main primitive type. 'Nuf said.
    // TODO: Field { constant: bool },
    Field,

    /// Custom / user-defined types
    Custom { module: Option<Ident>, name: Ident },

    /// This could be the same as Field, but we use this to also track the fact that it's a constant.
    // TODO: get rid of this type tho no?
    BigInt,

    /// An array of a fixed size.
    Array(Box<TyKind>, u32),

    /// A boolean (`true` or `false`).
    Bool,
    // Tuple(Vec<TyKind>),
    // Bool,
    // U8,
    // U16,
    // U32,
    // U64,
}

impl TyKind {
    pub fn match_expected(&self, expected: &TyKind) -> bool {
        match (self, expected) {
            (TyKind::BigInt, TyKind::Field) => true,
            (TyKind::Array(lhs, lhs_size), TyKind::Array(rhs, rhs_size)) => {
                lhs_size == rhs_size && lhs.match_expected(rhs)
            }
            (
                TyKind::Custom { module, name },
                TyKind::Custom {
                    module: expected_module,
                    name: expected_name,
                },
            ) => {
                module
                    .as_ref()
                    .zip(expected_module.as_ref())
                    .map_or(true, |(a, b)| a == b)
                    && name.value == expected_name.value
            }
            (x, y) if x == y => true,
            _ => false,
        }
    }

    pub fn same_as(&self, other: &TyKind) -> bool {
        match (self, other) {
            (TyKind::BigInt, TyKind::Field) | (TyKind::Field, TyKind::BigInt) => true,
            (TyKind::Array(lhs, lhs_size), TyKind::Array(rhs, rhs_size)) => {
                lhs_size == rhs_size && lhs.match_expected(rhs)
            }
            (
                TyKind::Custom { module, name },
                TyKind::Custom {
                    module: expected_module,
                    name: expected_name,
                },
            ) => {
                module
                    .as_ref()
                    .zip(expected_module.as_ref())
                    .map_or(true, |(a, b)| a == b)
                    && name.value == expected_name.value
            }
            (x, y) if x == y => true,
            _ => false,
        }
    }
}

impl Display for TyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TyKind::Custom { module, name } => {
                if let Some(module) = module {
                    write!(
                        f,
                        "a `{}` struct from module `{}`",
                        name.value, module.value
                    )
                } else {
                    write!(f, "a `{}` struct", name.value)
                }
            }
            TyKind::Field => write!(f, "Field"),
            TyKind::BigInt => write!(f, "BigInt"),
            TyKind::Array(ty, size) => write!(f, "[{}; {}]", ty, size),
            TyKind::Bool => write!(f, "Bool"),
        }
    }
}

impl Ty {
    pub fn reserved_types(module: Option<Ident>, name: Ident) -> TyKind {
        match name.value.as_ref() {
            "Field" | "Bool" if module.is_some() => {
                panic!("reserved types cannot be in a module (TODO: better error)")
            }
            "Field" => TyKind::Field,
            "Bool" => TyKind::Bool,
            _ => TyKind::Custom { module, name },
        }
    }

    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        let token = tokens.bump_err(ctx, ErrorKind::MissingType)?;
        match token.kind {
            // module::Type or Type
            // ^^^^^^^^^^^^    ^^^^
            TokenKind::Identifier(ty_name) => {
                let maybe_module = Ident::new(ty_name.clone(), token.span);
                let (module, name, _span) = if is_type(&ty_name) {
                    // Type
                    // ^^^^
                    (None, maybe_module, token.span)
                } else {
                    // module::Type
                    //       ^^
                    tokens.bump_expected(ctx, TokenKind::DoubleColon)?;

                    // module::Type
                    //         ^^^^
                    let (name, span) = match tokens.bump(ctx) {
                        Some(Token {
                            kind: TokenKind::Identifier(name),
                            span,
                        }) => (name, span),
                        _ => return Err(ctx.error(ErrorKind::MissingType, ctx.last_span())),
                    };
                    let name = Ident::new(name, span);
                    let span = token.span.merge_with(span);

                    (Some(maybe_module), name, span)
                };

                let ty_kind = Self::reserved_types(module, name);

                Ok(Self {
                    kind: ty_kind,
                    span: token.span,
                })
            }

            // array
            // [type; size]
            // ^
            TokenKind::LeftBracket => {
                let span = Span(token.span.0, 0);

                // [type; size]
                //   ^
                let ty = Ty::parse(ctx, tokens)?;

                // [type; size]
                //      ^
                tokens.bump_expected(ctx, TokenKind::SemiColon)?;

                // [type; size]
                //         ^
                let siz = tokens.bump_err(ctx, ErrorKind::InvalidToken)?;
                let siz: u32 = match siz.kind {
                    TokenKind::BigInt(s) => s
                        .parse()
                        .map_err(|_e| ctx.error(ErrorKind::InvalidArraySize, siz.span))?,
                    _ => {
                        return Err(ctx.error(
                            ErrorKind::ExpectedToken(TokenKind::BigInt("".to_string())),
                            siz.span,
                        ));
                    }
                };

                // [type; size]
                //            ^
                let right_paren = tokens.bump_expected(ctx, TokenKind::RightBracket)?;

                let span = span.merge_with(right_paren.span);

                Ok(Ty {
                    kind: TyKind::Array(Box::new(ty.kind), siz),
                    span,
                })
            }

            // unrecognized
            _ => Err(ctx.error(ErrorKind::InvalidType, token.span)),
        }
    }
}

//~
//~ ## Functions
//~
//~ Backus–Naur Form (BNF) grammar:
//~
//~ fn_sig ::= ident "(" param { "," param } ")" [ return_val ]
//~ return_val ::= "->" type
//~ param ::= { "pub" } ident ":" type
//~

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FnSig {
    pub name: FnNameDef,

    /// (pub, ident, type)
    pub arguments: Vec<FnArg>,

    pub return_type: Option<Ty>,
}

impl FnSig {
    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        let name = FnNameDef::parse(ctx, tokens)?;

        let arguments = Function::parse_args(ctx, tokens, name.self_name.as_ref())?;

        let return_type = Function::parse_fn_return_type(ctx, tokens)?;

        Ok(Self {
            name,
            arguments,
            return_type,
        })
    }
}

/// Any kind of text that can represent a type, a variable, a function name, etc.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ident {
    pub value: String,
    pub span: Span,
}

impl Ident {
    pub fn new(value: String, span: Span) -> Self {
        Self { value, span }
    }

    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        let token = tokens.bump_err(ctx, ErrorKind::MissingToken)?;
        match token.kind {
            TokenKind::Identifier(ident) => Ok(Self {
                value: ident,
                span: token.span,
            }),

            _ => Err(ctx.error(
                ErrorKind::ExpectedToken(TokenKind::Identifier("".to_string())),
                token.span,
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AttributeKind {
    Pub,
    Const,
}

impl AttributeKind {
    pub fn is_public(&self) -> bool {
        matches!(self, Self::Pub)
    }

    pub fn is_constant(&self) -> bool {
        matches!(self, Self::Const)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub kind: AttributeKind,
    pub span: Span,
}

impl Attribute {
    pub fn is_public(&self) -> bool {
        self.kind.is_public()
    }

    pub fn is_constant(&self) -> bool {
        self.kind.is_constant()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub sig: FnSig,

    pub body: Vec<Stmt>,

    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FnArg {
    pub name: Ident,
    pub typ: Ty,
    pub attribute: Option<Attribute>,
    pub span: Span,
}

impl FnArg {
    pub fn is_public(&self) -> bool {
        self.attribute
            .as_ref()
            .map(|attr| attr.is_public())
            .unwrap_or(false)
    }

    pub fn is_constant(&self) -> bool {
        self.attribute
            .as_ref()
            .map(|attr| attr.is_constant())
            .unwrap_or(false)
    }
}

/// Represents the name of a function.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FnNameDef {
    /// The name of the type that this function is implemented on.
    pub self_name: Option<Ident>,

    /// The name of the function.
    pub name: Ident,

    /// The span of the function.
    pub span: Span,
}

impl FnNameDef {
    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        // fn House.verify(   or   fn verify(
        //    ^^^^^                   ^^^^^
        let maybe_self_name = tokens.bump_ident(
            ctx,
            ErrorKind::InvalidFunctionSignature("expected function name"),
        )?;
        let span = maybe_self_name.span;

        // fn House.verify(
        //    ^^^^^
        if is_type(&maybe_self_name.value) {
            // fn House.verify(
            //         ^
            tokens.bump_expected(ctx, TokenKind::Dot)?;

            // fn House.verify(
            //          ^^^^^^
            let name = tokens.bump_ident(
                ctx,
                ErrorKind::InvalidFunctionSignature("expected function name"),
            )?;

            let span = span.merge_with(name.span);

            Ok(Self {
                self_name: Some(maybe_self_name),
                name,
                span,
            })
        } else {
            // fn verify(
            //    ^^^^^^
            Ok(Self {
                self_name: None,
                name: maybe_self_name,
                span,
            })
        }
    }
}

impl Function {
    pub fn is_main(&self) -> bool {
        self.sig.name.name.value == "main"
    }

    pub fn parse_args(
        ctx: &mut ParserCtx,
        tokens: &mut Tokens,
        self_name: Option<&Ident>,
    ) -> Result<Vec<FnArg>> {
        // (pub arg1: type1, arg2: type2)
        // ^
        tokens.bump_expected(ctx, TokenKind::LeftParen)?;

        // (pub arg1: type1, arg2: type2)
        //   ^
        let mut args = vec![];

        loop {
            // `pub arg1: type1`
            //   ^   ^
            let token = tokens.bump_err(
                ctx,
                ErrorKind::InvalidFunctionSignature("expected function arguments"),
            )?;

            let (attribute, arg_name) = match token.kind {
                TokenKind::RightParen => break,
                // public input
                TokenKind::Keyword(Keyword::Pub) => {
                    let arg_name = Ident::parse(ctx, tokens)?;
                    (
                        Some(Attribute {
                            kind: AttributeKind::Pub,
                            span: token.span,
                        }),
                        arg_name,
                    )
                }
                // constant input
                TokenKind::Keyword(Keyword::Const) => {
                    let arg_name = Ident::parse(ctx, tokens)?;
                    (
                        Some(Attribute {
                            kind: AttributeKind::Const,
                            span: token.span,
                        }),
                        arg_name,
                    )
                }
                // private input
                TokenKind::Identifier(name) => (
                    None,
                    Ident {
                        value: name,
                        span: token.span,
                    },
                ),
                _ => {
                    return Err(ctx.error(
                        ErrorKind::InvalidFunctionSignature("expected identifier"),
                        token.span,
                    ));
                }
            };

            // self takes no value
            let arg_typ = if arg_name.value == "self" {
                let self_name = self_name.ok_or_else(|| {
                    ctx.error(
                        ErrorKind::InvalidFunctionSignature(
                            "the `self` argument is only allowed in struct methods",
                        ),
                        arg_name.span,
                    )
                })?;

                if !args.is_empty() {
                    return Err(ctx.error(
                        ErrorKind::InvalidFunctionSignature("`self` must be the first argument"),
                        arg_name.span,
                    ));
                }

                Ty {
                    kind: TyKind::Custom {
                        module: None,
                        name: Ident::new(self_name.value.clone(), self_name.span),
                    },
                    span: self_name.span,
                }
            } else {
                // :
                tokens.bump_expected(ctx, TokenKind::Colon)?;

                // type
                Ty::parse(ctx, tokens)?
            };

            // , or )
            let separator = tokens.bump_err(
                ctx,
                ErrorKind::InvalidFunctionSignature("expected end of function or other argument"),
            )?;

            let span = if let Some(attr) = &attribute {
                if &arg_name.value == "self" {
                    return Err(ctx.error(ErrorKind::SelfHasAttribute, arg_name.span));
                } else {
                    attr.span.merge_with(arg_typ.span)
                }
            } else {
                if &arg_name.value == "self" {
                    arg_name.span
                } else {
                    arg_name.span.merge_with(arg_typ.span)
                }
            };

            let arg = FnArg {
                name: arg_name,
                typ: arg_typ,
                attribute,
                span,
            };
            args.push(arg);

            match separator.kind {
                // (pub arg1: type1, arg2: type2)
                //                 ^
                TokenKind::Comma => (),
                // (pub arg1: type1, arg2: type2)
                //                              ^
                TokenKind::RightParen => break,
                _ => {
                    return Err(ctx.error(
                        ErrorKind::InvalidFunctionSignature(
                            "expected end of function or other argument",
                        ),
                        separator.span,
                    ));
                }
            }
        }

        Ok(args)
    }

    pub fn parse_fn_return_type(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Option<Ty>> {
        match tokens.peek() {
            Some(Token {
                kind: TokenKind::RightArrow,
                ..
            }) => {
                tokens.bump(ctx);

                let return_type = Ty::parse(ctx, tokens)?;
                Ok(Some(return_type))
            }
            _ => Ok(None),
        }
    }

    pub fn parse_fn_body(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Vec<Stmt>> {
        let mut body = vec![];

        tokens.bump_expected(ctx, TokenKind::LeftCurlyBracket)?;

        loop {
            // end of the function
            let next_token = tokens.peek();
            if matches!(
                next_token,
                Some(Token {
                    kind: TokenKind::RightCurlyBracket,
                    ..
                })
            ) {
                tokens.bump(ctx);
                break;
            }

            // parse next statement
            let statement = Stmt::parse(ctx, tokens)?;
            body.push(statement);
        }

        Ok(body)
    }

    /// Parse a function, without the `fn` keyword.
    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        // ghetto way of getting the span of the function: get the span of the first token (name), then try to get the span of the last token
        let mut span = tokens
            .peek()
            .ok_or_else(|| {
                ctx.error(
                    ErrorKind::InvalidFunctionSignature("expected function name"),
                    ctx.last_span(),
                )
            })?
            .span;

        let name = FnNameDef::parse(ctx, tokens)?;
        let arguments = Self::parse_args(ctx, tokens, name.self_name.as_ref())?;
        let return_type = Self::parse_fn_return_type(ctx, tokens)?;
        let body = Self::parse_fn_body(ctx, tokens)?;

        // here's the last token, that is if the function is not empty (maybe we should disallow empty functions?)

        if let Some(t) = body.last() {
            span.1 = (t.span.0 + t.span.1) - span.0;
        } else {
            return Err(ctx.error(
                ErrorKind::InvalidFunctionSignature("expected function body"),
                ctx.last_span(),
            ));
        }

        let func = Self {
            sig: FnSig {
                name,
                arguments,
                return_type,
            },
            body,
            span,
        };

        Ok(func)
    }
}

// TODO: enforce snake_case?
pub fn is_valid_fn_name(name: &str) -> bool {
    if let Some(first_char) = name.chars().next() {
        // first character is not a number
        (first_char.is_alphabetic() || first_char == '_')
            // first character is lowercase
            && first_char.is_lowercase()
            // all other characters are alphanumeric or underscore
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    } else {
        false
    }
}

// TODO: enforce CamelCase?
pub fn is_valid_fn_type(name: &str) -> bool {
    if let Some(first_char) = name.chars().next() {
        // first character is not a number or alpha
        first_char.is_alphabetic()
            // first character is uppercase
            && first_char.is_uppercase()
            // all other characters are alphanumeric or underscore
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    } else {
        false
    }
}

//
// ## Statements
//
//~ statement ::=
//~     | "let" ident "=" expr ";"
//~     | expr ";"
//~     | "return" expr ";"
//~
//~ where an expression is allowed only if it is a function call that does not return a value.
//~
//~ Actually currently we don't implement it this way.
//~ We don't expect an expression to be a statement,
//~ but a well defined function call:
//~
//~ fn_call ::= path "(" [ expr { "," expr } ] ")"
//~ path ::= ident { "::" ident }
//~

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: u32,
    pub end: u32,
    pub span: Span,
}

impl Range {
    pub fn range(&self) -> std::ops::Range<u32> {
        self.start..self.end
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StmtKind {
    Assign {
        mutable: bool,
        lhs: Ident,
        rhs: Box<Expr>,
    },
    Expr(Box<Expr>),
    Return(Box<Expr>),
    Comment(String),

    // `for var in 0..10 { <body> }`
    ForLoop {
        var: Ident,
        range: Range,
        body: Vec<Stmt>,
    },
}

impl Stmt {
    /// Returns a list of statement parsed until seeing the end of a block (`}`).
    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        match tokens.peek() {
            None => Err(ctx.error(ErrorKind::InvalidStatement, ctx.last_span())),
            // assignment
            Some(Token {
                kind: TokenKind::Keyword(Keyword::Let),
                span,
            }) => {
                let mut span = span;
                tokens.bump(ctx);

                // let mut x = 5;
                //     ^^^

                let mutable = if matches!(
                    tokens.peek(),
                    Some(Token {
                        kind: TokenKind::Keyword(Keyword::Mut),
                        ..
                    })
                ) {
                    tokens.bump(ctx);
                    true
                } else {
                    false
                };

                // let mut x = 5;
                //         ^
                let lhs = Ident::parse(ctx, tokens)?;

                // let mut x = 5;
                //           ^
                tokens.bump_expected(ctx, TokenKind::Equal)?;

                // let mut x = 5;
                //             ^
                let rhs = Box::new(Expr::parse(ctx, tokens)?);

                span.1 = rhs.span.1 + rhs.span.0 - span.0;

                // let mut x = 5;
                //              ^
                tokens.bump_expected(ctx, TokenKind::SemiColon)?;

                //
                Ok(Stmt {
                    kind: StmtKind::Assign { mutable, lhs, rhs },
                    span,
                })
            }

            // for loop
            Some(Token {
                kind: TokenKind::Keyword(Keyword::For),
                span,
            }) => {
                tokens.bump(ctx);

                // for i in 0..5 { ... }
                //     ^
                let var = Ident::parse(ctx, tokens)?;

                // for i in 0..5 { ... }
                //       ^^
                tokens.bump_expected(ctx, TokenKind::Keyword(Keyword::In))?;

                // for i in 0..5 { ... }
                //          ^
                let (start, start_span) = match tokens.bump(ctx) {
                    Some(Token {
                        kind: TokenKind::BigInt(n),
                        span,
                    }) => {
                        let start: u32 = n
                            .parse()
                            .map_err(|_e| ctx.error(ErrorKind::InvalidRangeSize, span))?;
                        (start, span)
                    }
                    _ => {
                        return Err(ctx.error(
                            ErrorKind::ExpectedToken(TokenKind::BigInt("".to_string())),
                            ctx.last_span(),
                        ))
                    }
                };

                // for i in 0..5 { ... }
                //           ^^
                tokens.bump_expected(ctx, TokenKind::DoubleDot)?;

                // for i in 0..5 { ... }
                //             ^
                let (end, end_span) = match tokens.bump(ctx) {
                    Some(Token {
                        kind: TokenKind::BigInt(n),
                        span,
                    }) => {
                        let end: u32 = n
                            .parse()
                            .map_err(|_e| ctx.error(ErrorKind::InvalidRangeSize, span))?;
                        (end, span)
                    }
                    _ => {
                        return Err(ctx.error(
                            ErrorKind::ExpectedToken(TokenKind::BigInt("".to_string())),
                            ctx.last_span(),
                        ))
                    }
                };

                let range = Range {
                    start,
                    end,
                    span: start_span.merge_with(end_span),
                };

                // for i in 0..5 { ... }
                //               ^
                tokens.bump_expected(ctx, TokenKind::LeftCurlyBracket)?;

                // for i in 0..5 { ... }
                //                 ^^^
                let mut body = vec![];

                loop {
                    // for i in 0..5 { ... }
                    //                     ^
                    let next_token = tokens.peek();
                    if matches!(
                        next_token,
                        Some(Token {
                            kind: TokenKind::RightCurlyBracket,
                            ..
                        })
                    ) {
                        tokens.bump(ctx);
                        break;
                    }

                    // parse next statement
                    // TODO: should we prevent `return` here?
                    // TODO: in general, do we prevent early returns atm?
                    let statement = Stmt::parse(ctx, tokens)?;
                    body.push(statement);
                }

                //
                Ok(Stmt {
                    kind: StmtKind::ForLoop { var, range, body },
                    span,
                })
            }

            // if/else
            Some(Token {
                kind: TokenKind::Keyword(Keyword::If),
                span: _,
            }) => {
                // TODO: wait, this should be implemented as an expresssion! not a statement
                panic!("if statements are not implemented yet. Use if expressions instead (e.g. `x = if cond {{ 1 }} else {{ 2 }};`)");
            }

            // return
            Some(Token {
                kind: TokenKind::Keyword(Keyword::Return),
                span,
            }) => {
                tokens.bump(ctx);

                // return xx;
                //        ^^
                let expr = Expr::parse(ctx, tokens)?;

                // return xx;
                //          ^
                tokens.bump_expected(ctx, TokenKind::SemiColon)?;

                Ok(Stmt {
                    kind: StmtKind::Return(Box::new(expr)),
                    span,
                })
            }

            // comment
            Some(Token {
                kind: TokenKind::Comment(c),
                span,
            }) => {
                tokens.bump(ctx);
                Ok(Stmt {
                    kind: StmtKind::Comment(c),
                    span,
                })
            }

            // statement expression (like function call)
            _ => {
                let expr = Expr::parse(ctx, tokens)?;
                let span = expr.span;

                tokens.bump_expected(ctx, TokenKind::SemiColon)?;

                Ok(Stmt {
                    kind: StmtKind::Expr(Box::new(expr)),
                    span,
                })
            }
        }
    }
}

//
// Scope
//

// TODO: where do I enforce that there's not several `use` with the same module name? or several functions with the same names? I guess that's something I need to enforce in any scope anyway...
#[derive(Debug)]

/// Things you can have in a scope (including the root scope).
pub struct Root {
    pub kind: RootKind,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsePath {
    pub module: Ident,
    pub submodule: Ident,
    pub span: Span,
}

impl From<&UsePath> for UserRepo {
    fn from(path: &UsePath) -> Self {
        UserRepo {
            user: path.module.value.clone(),
            repo: path.submodule.value.clone(),
        }
    }
}

impl Display for UsePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.module.value, self.submodule.value)
    }
}

impl UsePath {
    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        let module = tokens.bump_ident(
            ctx,
            ErrorKind::InvalidPath("wrong path: expected a module (TODO: better error"),
        )?;
        let span = module.span;

        tokens.bump_expected(ctx, TokenKind::DoubleColon)?; // ::

        let submodule = tokens.bump_ident(
            ctx,
            ErrorKind::InvalidPath(
                "wrong path: expected a submodule after `::` (TODO: better error",
            ),
        )?;

        let span = span.merge_with(submodule.span);
        Ok(UsePath {
            module,
            submodule,
            span,
        })
    }
}

#[derive(Debug)]
pub enum RootKind {
    Use(UsePath),
    Function(Function),
    Comment(String),
    Struct(Struct),
    Const(Const),
}

//
// Const
//

#[derive(Debug)]
pub struct Const {
    pub name: Ident,
    pub value: Field,
    pub span: Span,
}

impl Const {
    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        // const foo = 42;
        //       ^^^
        let name = Ident::parse(ctx, tokens)?;

        // const foo = 42;
        //           ^
        tokens.bump_expected(ctx, TokenKind::Equal)?;

        // const foo = 42;
        //             ^^
        let value = Expr::parse(ctx, tokens)?;
        let value = match &value.kind {
            ExprKind::BigInt(s) => s
                .parse()
                .map_err(|_e| ctx.error(ErrorKind::InvalidField(s.clone()), value.span))?,
            _ => {
                return Err(ctx.error(ErrorKind::InvalidConstType, value.span));
            }
        };

        // const foo = 42;
        //               ^
        tokens.bump_expected(ctx, TokenKind::SemiColon)?;

        //
        let span = name.span;
        Ok(Const { name, value, span })
    }
}

//
// Custom Struct
//

#[derive(Debug)]
pub struct Struct {
    //pub attribute: Attribute,
    pub name: CustomType,
    pub fields: Vec<(Ident, Ty)>,
    pub span: Span,
}

impl Struct {
    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        // ghetto way of getting the span of the function: get the span of the first token (name), then try to get the span of the last token
        let span = tokens
            .peek()
            .ok_or_else(|| {
                ctx.error(
                    ErrorKind::InvalidFunctionSignature("expected function name"),
                    ctx.last_span(),
                )
            })?
            .span;

        // struct Foo { a: Field, b: Field }
        //        ^^^

        let name = CustomType::parse(ctx, tokens)?;

        // struct Foo { a: Field, b: Field }
        //            ^
        tokens.bump_expected(ctx, TokenKind::LeftCurlyBracket)?;

        let mut fields = vec![];
        loop {
            // struct Foo { a: Field, b: Field }
            //                                 ^
            if let Some(Token {
                kind: TokenKind::RightCurlyBracket,
                ..
            }) = tokens.peek()
            {
                tokens.bump(ctx);
                break;
            }
            // struct Foo { a: Field, b: Field }
            //              ^
            let field_name = Ident::parse(ctx, tokens)?;

            // struct Foo { a: Field, b: Field }
            //               ^
            tokens.bump_expected(ctx, TokenKind::Colon)?;

            // struct Foo { a: Field, b: Field }
            //                 ^^^^^
            let field_ty = Ty::parse(ctx, tokens)?;
            fields.push((field_name, field_ty));

            // struct Foo { a: Field, b: Field }
            //                      ^          ^
            match tokens.peek() {
                Some(Token {
                    kind: TokenKind::Comma,
                    ..
                }) => {
                    tokens.bump(ctx);
                }
                Some(Token {
                    kind: TokenKind::RightCurlyBracket,
                    ..
                }) => {
                    tokens.bump(ctx);
                    break;
                }
                _ => {
                    return Err(
                        ctx.error(ErrorKind::ExpectedToken(TokenKind::Comma), ctx.last_span())
                    )
                }
            }
        }

        // figure out the span
        let span = span.merge_with(ctx.last_span());

        //
        Ok(Struct { name, fields, span })
    }
}

//
// CustomType
//

#[derive(Debug)]
pub struct CustomType {
    pub value: String,
    pub span: Span,
}

impl CustomType {
    pub fn parse(ctx: &mut ParserCtx, tokens: &mut Tokens) -> Result<Self> {
        let ty_name = tokens.bump_ident(ctx, ErrorKind::InvalidType)?;

        if !is_type(&ty_name.value) {
            panic!("type name should start with uppercase letter (TODO: better error");
        }

        // make sure that this type is allowed
        if !matches!(
            Ty::reserved_types(None, ty_name.clone()),
            TyKind::Custom { .. }
        ) {
            return Err(ctx.error(ErrorKind::ReservedType(ty_name.value), ty_name.span));
        }

        Ok(Self {
            value: ty_name.value,
            span: ty_name.span,
        })
    }
}