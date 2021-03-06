use std::collections::{BTreeMap, HashMap, VecDeque};
use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::Arc;

use snafu::ResultExt;

use libeir_diagnostics::*;
use libeir_util_parse::{ErrorReceiver, ErrorReceiverTee, Source};

use crate::lexer::Lexer;
use crate::lexer::{symbols, DelayedSubstitution, IdentToken, Lexed, LexicalToken, Symbol, Token};
use crate::parser::Parser;

use super::errors;
use super::macros::Stringify;
use super::token_reader::{TokenBufferReader, TokenReader, TokenStreamReader};
use super::{Directive, MacroCall, MacroContainer, MacroDef, MacroIdent};
use super::{Preprocessed, PreprocessorError, Result as PResult};

type Errors<'a> = ErrorReceiverTee<'a, PreprocessorError, PreprocessorError>;

macro_rules! error_into {
    ($errors:expr, $result:expr) => {
        match $result {
            Ok(inner) => Ok(inner),
            Err(error) => {
                $errors.error(error.into());
                Err(())
            }
        }
    };
}

pub struct Preprocessor<'a, Reader: TokenReader> {
    errors: Errors<'a>,
    codemap: Arc<CodeMap>,
    reader: Reader,
    can_directive_start: bool,
    directives: BTreeMap<SourceIndex, Directive>,
    code_paths: VecDeque<PathBuf>,
    include_paths: VecDeque<PathBuf>,
    branches: Vec<Branch>,
    macros: MacroContainer,
    macro_calls: BTreeMap<SourceIndex, MacroCall>,
    expanded_tokens: VecDeque<LexicalToken>,
    warnings_as_errors: bool,
    no_warn: bool,
}
impl<'a, S> Preprocessor<'a, TokenStreamReader<S>>
where
    S: Source,
{
    pub fn new(parser: &Parser, tokens: Lexer<S>, errors: Errors<'a>) -> Self {
        let reader = TokenStreamReader::new(parser.codemap.clone(), tokens);
        let code_paths = parser.config.code_paths.clone();
        let include_paths = parser.config.include_paths.clone();

        let mut macros = match parser.config.macros {
            None => MacroContainer::new(),
            Some(ref macros) => macros.clone(),
        };
        macros.insert(
            MacroIdent::Const(Symbol::intern("FUNCTION_NAME")),
            MacroDef::DelayedSubstitution(DelayedSubstitution::FunctionName),
        );
        macros.insert(
            MacroIdent::Const(Symbol::intern("FUNCTION_ARITY")),
            MacroDef::DelayedSubstitution(DelayedSubstitution::FunctionArity),
        );

        Preprocessor {
            errors,
            codemap: parser.codemap.clone(),
            reader,
            can_directive_start: true,
            directives: BTreeMap::new(),
            code_paths,
            include_paths,
            branches: Vec::new(),
            macros,
            macro_calls: BTreeMap::new(),
            expanded_tokens: VecDeque::new(),
            warnings_as_errors: parser.config.warnings_as_errors,
            no_warn: parser.config.no_warn,
        }
    }
}
impl<'a, R, S> Preprocessor<'a, R>
where
    R: TokenReader<Source = S>,
{
    fn clone_with(&self, tokens: VecDeque<Lexed>) -> Preprocessor<TokenBufferReader> {
        let codemap = self.codemap.clone();
        let reader = TokenBufferReader::new(codemap.clone(), tokens);
        Preprocessor {
            errors: self.errors.clone(),
            codemap,
            reader,
            can_directive_start: false,
            directives: BTreeMap::new(),
            code_paths: self.code_paths.clone(),
            include_paths: self.include_paths.clone(),
            branches: Vec::new(),
            macros: self.macros.clone(),
            macro_calls: BTreeMap::new(),
            expanded_tokens: VecDeque::new(),
            warnings_as_errors: self.warnings_as_errors,
            no_warn: self.no_warn,
        }
    }

    fn ignore(&self) -> bool {
        self.branches.iter().any(|b| !b.entered)
    }

    fn next_token(&mut self) -> Result<Option<LexicalToken>, ()> {
        loop {
            if let Some(token) = self.expanded_tokens.pop_front() {
                return Ok(Some(token));
            }
            if self.can_directive_start {
                match self.try_read_directive()? {
                    Some(Directive::Module(d)) => {
                        // We need to expand this directive back to a token stream for the parser
                        self.expanded_tokens = d.expand();
                        // Otherwise treat it like other directives
                        self.directives
                            .insert(d.span().start(), Directive::Module(d));
                        continue;
                    }
                    Some(d) => {
                        self.directives.insert(d.span().start(), d);
                        continue;
                    }
                    None => (),
                }
            }
            if !self.ignore() {
                if let Some(m) =
                    error_into!(self.errors, self.reader.try_read_macro_call(&self.macros))?
                {
                    self.macro_calls.insert(m.span().start(), m.clone());
                    self.expanded_tokens = error_into!(self.errors, self.expand_macro(m))?;
                    continue;
                }
            }
            if let Some(token) = error_into!(self.errors, self.reader.try_read_token())? {
                if self.ignore() {
                    continue;
                }
                if let LexicalToken(_, Token::Dot, _) = token {
                    self.can_directive_start = true;
                } else {
                    self.can_directive_start = false;
                }
                return Ok(Some(token));
            } else {
                break;
            }
        }
        Ok(None)
    }

    fn expand_macro(&self, call: MacroCall) -> PResult<VecDeque<LexicalToken>> {
        if let Some(expanded) = self.try_expand_predefined_macro(&call)? {
            Ok(vec![expanded].into())
        } else {
            self.expand_userdefined_macro(call)
        }
    }

    fn try_expand_predefined_macro(&self, call: &MacroCall) -> PResult<Option<LexicalToken>> {
        let expanded = match call.name().as_str().get() {
            "FILE" => {
                let span = call.span();
                let source_id = span.source_id();
                let current = span.start();
                let file = self.codemap.get(source_id).unwrap();
                let filename = file.name().to_string();
                LexicalToken(
                    current,
                    Token::String(Symbol::intern(&filename)),
                    span.end(),
                )
            }
            "LINE" => {
                let span = call.span();
                let source_id = span.source_id();
                let current = span.start();
                let file = self.codemap.get(source_id).unwrap();
                let line = file.line_index(current.index()).to_usize() as i64;
                LexicalToken(current, Token::Integer(line.into()), span.end())
            }
            "MACHINE" => {
                let span = call.span();
                let current = span.start();
                LexicalToken(current, Token::Atom(Symbol::intern("Lumen")), span.end())
            }
            _ => return Ok(None),
        };
        Ok(Some(expanded))
    }

    fn expand_userdefined_macro(&self, call: MacroCall) -> PResult<VecDeque<LexicalToken>> {
        let definition = match self.macros.get(&call) {
            None => return Err(PreprocessorError::UndefinedMacro { call }),
            Some(def) => def,
        };
        match *definition {
            MacroDef::Dynamic(ref replacement) => Ok(replacement.clone().into()),
            MacroDef::String(ref s) => Ok(vec![LexicalToken(
                SourceIndex::UNKNOWN,
                Token::String(s.clone()),
                SourceIndex::UNKNOWN,
            )]
            .into()),
            MacroDef::Boolean(true) => Ok(vec![LexicalToken(
                SourceIndex::UNKNOWN,
                Token::Atom(symbols::True),
                SourceIndex::UNKNOWN,
            )]
            .into()),
            MacroDef::Boolean(false) => Ok(VecDeque::new()),
            MacroDef::Static(ref def) => {
                let arity = def.variables.as_ref().map(|v| v.len()).unwrap_or(0);
                let argc = call.args.as_ref().map(|a| a.len()).unwrap_or(0);
                if arity != argc {
                    let err = format!(
                        "expected {} arguments at call site, but given {}",
                        arity, argc
                    );
                    return Err(PreprocessorError::BadMacroCall {
                        call,
                        def: definition.clone(),
                        reason: err,
                    });
                }
                let bindings = def
                    .variables
                    .as_ref()
                    .iter()
                    .flat_map(|i| i.iter().map(|v| v.symbol()))
                    .zip(
                        call.args
                            .iter()
                            .flat_map(|i| i.iter().map(|a| &a.tokens[..])),
                    )
                    .collect::<HashMap<_, _>>();
                let expanded = self.expand_replacement(bindings, &def.replacement)?;
                Ok(expanded)
            }
            MacroDef::DelayedSubstitution(subst) => Ok(vec![LexicalToken(
                call.span().start(),
                Token::DelayedSubstitution(subst),
                call.span().end(),
            )]
            .into()),
        }
    }

    fn expand_replacement(
        &self,
        bindings: HashMap<Symbol, &[LexicalToken]>,
        replacement: &[LexicalToken],
    ) -> PResult<VecDeque<LexicalToken>> {
        let mut expanded = VecDeque::new();
        let replacement_tokens: VecDeque<_> = replacement.iter().map(|t| Ok(t.clone())).collect();
        let mut reader = TokenBufferReader::new(self.codemap.clone(), replacement_tokens);

        loop {
            if let Some(call) = reader.try_read_macro_call(&self.macros)? {
                let nested = self.expand_macro(call)?;
                for token in nested.into_iter().rev() {
                    reader.unread_token(token);
                }
            } else if let Some(stringify) = reader.try_read::<Stringify>()? {
                let tokens = match bindings.get(&stringify.name.symbol()) {
                    None => {
                        return Err(PreprocessorError::UndefinedStringifyMacro { call: stringify })
                    }
                    Some(tokens) => tokens,
                };
                let string = tokens.iter().map(|t| t.to_string()).collect::<String>();
                let span = tokens[0].span();
                let start = span.start();
                let end = span.end();
                let token = (start, Token::String(Symbol::intern(&string)), end);
                expanded.push_back(token.into());
            } else if let Some(token) = reader.try_read_token()? {
                match IdentToken::try_from(token.clone()) {
                    Ok(ident) => match bindings.get(&ident.symbol()) {
                        Some(value) => {
                            let nested = self.expand_replacement(HashMap::new(), value)?;
                            expanded.extend(nested);
                            continue;
                        }
                        None => (),
                    },
                    Err(_) => (),
                }
                expanded.push_back(token);
            } else {
                break;
            }
        }
        Ok(expanded)
    }

    fn try_read_directive(&mut self) -> Result<Option<Directive>, ()> {
        let directive: Directive =
            if let Some(directive) = error_into!(self.errors, self.reader.try_read())? {
                directive
            } else {
                return Ok(None);
            };

        let ignore = self.ignore();
        match directive {
            Directive::Module(ref d) => {
                self.macros.insert(
                    MacroIdent::Const(symbols::ModuleCapital),
                    MacroDef::String(d.name.symbol()),
                );
                self.macros.insert(
                    MacroIdent::Const(symbols::ModuleStringCapital),
                    MacroDef::String(d.name.symbol()),
                );
            }
            Directive::Include(ref d) if !ignore => {
                let path = error_into!(
                    self.errors,
                    d.include(&self.include_paths).context(errors::BadDirective)
                )?;
                error_into!(self.errors, self.reader.inject_include(path))?;
            }
            Directive::IncludeLib(ref d) if !ignore => {
                let path = error_into!(
                    self.errors,
                    d.include_lib(&self.code_paths)
                        .context(errors::BadDirective)
                )?;
                error_into!(self.errors, self.reader.inject_include(path))?;
            }
            Directive::Define(ref d) if !ignore => {
                self.macros.insert(d, MacroDef::Static(d.clone()));
            }
            Directive::Undef(ref d) if !ignore => {
                self.macros.undef(&d.name());
            }
            Directive::Ifdef(ref d) => {
                let entered = self.macros.defined(&d.name());
                self.branches.push(Branch::new(entered));
            }
            Directive::If(ref d) => {
                let entered = self.eval_conditional(d.span(), d.condition.clone())?;
                self.branches.push(Branch::new(entered));
            }
            Directive::Ifndef(ref d) => {
                let entered = !self.macros.defined(&d.name());
                self.branches.push(Branch::new(entered));
            }
            Directive::Else(_) => match self.branches.last_mut() {
                None => {
                    return error_into!(
                        self.errors,
                        Err(PreprocessorError::OrphanedElse { directive })
                    )
                }
                Some(branch) => {
                    match branch.switch_to_else_branch() {
                        Err(_) => {
                            return error_into!(
                                self.errors,
                                Err(PreprocessorError::OrphanedElse { directive })
                            )
                        }
                        Ok(_) => (),
                    };
                }
            },
            Directive::Elif(ref d) => {
                // Treat this like -endif followed by -if(Cond)
                match self.branches.pop() {
                    None => {
                        return error_into!(
                            self.errors,
                            Err(PreprocessorError::OrphanedElse { directive })
                        )
                    }
                    Some(_) => {
                        let entered = self.eval_conditional(d.span(), d.condition.clone())?;
                        self.branches.push(Branch::new(entered));
                    }
                }
            }
            Directive::Endif(_) => match self.branches.pop() {
                None => {
                    return error_into!(
                        self.errors,
                        Err(PreprocessorError::OrphanedEnd { directive })
                    )
                }
                Some(_) => (),
            },
            Directive::Error(ref d) if !ignore => {
                let span = d.span();
                let err = d.message.symbol().as_str().get().to_string();
                return error_into!(
                    self.errors,
                    Err(PreprocessorError::CompilerError {
                        span: Some(span),
                        reason: err
                    })
                );
            }
            Directive::Warning(ref d) if !ignore => {
                if self.no_warn {
                    return Ok(Some(directive));
                }
                if self.warnings_as_errors {
                    let span = d.span();
                    let err = d.message.symbol().as_str().get().to_string();
                    return error_into!(
                        self.errors,
                        Err(PreprocessorError::CompilerError {
                            span: Some(span),
                            reason: err
                        })
                    );
                }
                let span = d.span();
                let warn = d.message.symbol().as_str().get();
                let diag = Diagnostic::warning()
                    .with_message("found warning directive")
                    .with_labels(vec![
                        Label::primary(span.source_id(), span).with_message(warn)
                    ]);
                self.warning_diagnostic(diag);
            }
            Directive::File(ref f) if !ignore => {
                // TODO
                println!("TODO file directive {}", f);
            }
            _ => {}
        }
        Ok(Some(directive))
    }

    fn warning_diagnostic(&self, diagnostic: Diagnostic) {
        use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
        use codespan_reporting::term::*;

        let config = Config::default();
        let mut out = StandardStream::stderr(ColorChoice::Always);
        term::emit(&mut out, &config, &*self.codemap, &diagnostic).unwrap();
    }

    fn eval_conditional(
        &mut self,
        span: SourceSpan,
        condition: VecDeque<Lexed>,
    ) -> Result<bool, ()> {
        use crate::lexer::Ident;
        use crate::parser::ast::{Expr, Literal};
        use crate::parser::Parse;
        use crate::preprocessor::evaluator;

        let result = {
            let mut adapter = self.errors.make_adapter(
                move |v| PreprocessorError::ParseError {
                    span,
                    inner: Box::new(v),
                },
                move |v| PreprocessorError::ParseError {
                    span,
                    inner: Box::new(v),
                },
            );
            let pp = self.clone_with(condition);
            // TODO add adapter which adds PreprocessorError between
            //let adapter = ErrorReceiverAdapter::new(
            //    &mut self.errors,
            //);
            Expr::parse_tokens(&mut adapter, pp)
        };
        match evaluator::eval(result?) {
            Ok(Expr::Literal(Literal::Atom(_, Ident { ref name, .. })))
                if *name == symbols::True =>
            {
                Ok(true)
            }
            Ok(Expr::Literal(Literal::Atom(_, Ident { ref name, .. })))
                if *name == symbols::False =>
            {
                Ok(false)
            }
            Err(err) => {
                self.errors.error(err.into());
                return Err(());
            }
            _other => {
                self.errors
                    .error(PreprocessorError::InvalidConditional { span }.into());
                return Err(());
            }
        }
    }
}

impl<'a, R, S> Iterator for Preprocessor<'a, R>
where
    R: TokenReader<Source = S>,
{
    type Item = Preprocessed;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_token() {
            Err(()) => Some(Err(())),
            Ok(None) => None,
            Ok(Some(token)) => Some(Ok(token.into())),
        }
    }
}

#[derive(Debug)]
struct Branch {
    pub then_branch: bool,
    pub entered: bool,
}
impl Branch {
    pub fn new(entered: bool) -> Self {
        Branch {
            then_branch: true,
            entered,
        }
    }
    pub fn switch_to_else_branch(&mut self) -> Result<(), ()> {
        if !self.then_branch {
            return Err(());
        }
        self.then_branch = false;
        self.entered = !self.entered;
        Ok(())
    }
}
