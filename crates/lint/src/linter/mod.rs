mod early;
mod late;

pub use early::{EarlyLintPass, EarlyLintVisitor};
pub use late::{LateLintPass, LateLintVisitor};

use foundry_compilers::Language;
use foundry_config::lint::Severity;
use solar_interface::{
    Session, Span,
    diagnostics::{DiagBuilder, DiagId, DiagMsg, MultiSpan, Style},
};
use solar_sema::ParsingContext;
use std::path::PathBuf;

use crate::inline_config::InlineConfig;

/// Trait representing a generic linter for analyzing and reporting issues in smart contract source
/// code files. A linter can be implemented for any smart contract language supported by Foundry.
///
/// # Type Parameters
///
/// - `Language`: Represents the target programming language. Must implement the [`Language`] trait.
/// - `Lint`: Represents the types of lints performed by the linter. Must implement the [`Lint`]
///   trait.
///
/// # Required Methods
///
/// - `init`: Creates a new solar `Session` with the appropriate linter configuration.
/// - `early_lint`: Scans the source files (using the AST) emitting a diagnostic for lints found.
/// - `late_lint`: Scans the source files (using the HIR) emitting a diagnostic for lints found.
pub trait Linter: Send + Sync + Clone {
    type Language: Language;
    type Lint: Lint;

    fn init(&self) -> Session;
    fn early_lint<'sess>(&self, input: &[PathBuf], pcx: ParsingContext<'sess>);
    fn late_lint<'sess>(&self, input: &[PathBuf], pcx: ParsingContext<'sess>);
}

pub trait Lint {
    fn id(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn description(&self) -> &'static str;
    fn help(&self) -> &'static str;
}

pub struct LintContext<'s> {
    sess: &'s Session,
    with_description: bool,
    pub inline_config: InlineConfig,
    active_lints: Vec<&'static str>,
}

impl<'s> LintContext<'s> {
    pub fn new(
        sess: &'s Session,
        with_description: bool,
        config: InlineConfig,
        active_lints: Vec<&'static str>,
    ) -> Self {
        Self { sess, with_description, inline_config: config, active_lints }
    }

    pub fn session(&self) -> &'s Session {
        self.sess
    }

    // Helper method to check if a lint id is enabled.
    //
    // For performance reasons, some passes check several lints at once. Thus, this method is
    // required to avoid unintended warnings.
    pub fn is_lint_enabled(&self, id: &'static str) -> bool {
        self.active_lints.contains(&id)
    }

    /// Helper method to emit diagnostics easily from passes
    pub fn emit<L: Lint>(&self, lint: &'static L, span: Span) {
        if self.inline_config.is_disabled(span, lint.id()) || !self.is_lint_enabled(lint.id()) {
            return;
        }

        let desc = if self.with_description { lint.description() } else { "" };
        let diag: DiagBuilder<'_, ()> = self
            .sess
            .dcx
            .diag(lint.severity().into(), desc)
            .code(DiagId::new_str(lint.id()))
            .span(MultiSpan::from_span(span))
            .help(lint.help());

        diag.emit();
    }

    /// Emit a diagnostic with a code fix proposal.
    ///
    /// For Diff snippets, if no span is provided, it will use the lint's span.
    /// If unable to get code from the span, it will fall back to a Block snippet.
    pub fn emit_with_fix<L: Lint>(&self, lint: &'static L, span: Span, snippet: Snippet) {
        if self.inline_config.is_disabled(span, lint.id()) || !self.is_lint_enabled(lint.id()) {
            return;
        }

        // Convert the snippet to ensure we have the appropriate type
        let snippet = match snippet {
            Snippet::Diff { desc, span: diff_span, add } => {
                // Use the provided span or fall back to the lint span
                let target_span = diff_span.unwrap_or(span);

                // Check if we can get the original code
                if self.span_to_snippet(target_span).is_some() {
                    Snippet::Diff { desc, span: Some(target_span), add }
                } else {
                    // Fallback to a Block snippet if we can't get the source
                    Snippet::Block { desc, code: add }
                }
            }
            other => other,
        };

        let desc = if self.with_description { lint.description() } else { "" };
        let mut diag: DiagBuilder<'_, ()> = self
            .sess
            .dcx
            .diag(lint.severity().into(), desc)
            .code(DiagId::new_str(lint.id()))
            .span(MultiSpan::from_span(span))
            .help(lint.help());

        // Add the snippet as notes
        for (note, _style) in snippet.to_note(self) {
            diag = diag.note(note.clone());
        }

        diag.emit();
    }

    /// Helper method to get code from spans
    pub fn span_to_snippet(&self, span: Span) -> Option<String> {
        self.sess.source_map().span_to_snippet(span).ok()
    }

    /// Extracts the character at the byte `offset` of a span.
    /// Returns `0` (null character) if offset is out-of-bounds
    pub fn span_char_at_offset(&self, span: Span, offset: usize) -> char {
        let file = self.sess.source_map().lookup_source_file(span.lo());
        let lo = span.lo().to_usize();
        if let Some(global_offset) = lo.checked_add(offset) {
            if global_offset < file.end_position().to_usize() {
                return file.src
                    .chars()
                    .nth(global_offset - file.start_pos.to_usize())
                    .unwrap_or_default();
            }
        }

        0 as char
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Snippet {
    /// A standalone block of code. Used for showing examples without suggesting a fix.
    Block {
        /// An optional description displayed above the code block.
        desc: Option<&'static str>,
        /// The source code to display. Multi-line strings should include newlines.
        code: String,
    },

    /// A proposed code change, displayed as a diff. Used to suggest replacements, showing the code
    /// to be removed (from `span`) and the code to be added (from `add`).
    Diff {
        /// An optional description displayed above the diff.
        desc: Option<&'static str>,
        /// The `Span` of the source code to be removed. Note that, if uninformed,
        /// `fn emit_with_fix()` falls back to the lint span.
        span: Option<Span>,
        /// The replacement code to be suggested.
        add: String,
    },
}

impl Snippet {
    pub fn to_note(self, ctx: &LintContext<'_>) -> Vec<(DiagMsg, Style)> {
        match self {
            Self::Block { desc, code } => {
                let mut notes = Vec::new();

                if let Some(desc) = desc {
                    notes.push((desc.into(), Style::NoStyle));
                }

                // If the code contains newlines, display as a multi-line block
                if code.contains('\n') {
                    notes.push((format!("\n{code}").into(), Style::NoStyle));
                } else {
                    notes.push((format!("`{code}`").into(), Style::NoStyle));
                }

                notes
            }
            Self::Diff { desc, span, add } => {
                let mut notes = Vec::new();

                if let Some(desc) = desc {
                    notes.push((desc.into(), Style::NoStyle));
                }

                if let Some(span) = span {
                    if let Some(original) = ctx.span_to_snippet(span) {
                        // Display as a diff: - original, + replacement
                        let diff = if original.contains('\n') || add.contains('\n') {
                            format!("\n- {original}\n+ {add}")
                        } else {
                            format!("`- {original}` `+ {add}`")
                        };
                        notes.push((diff.into(), Style::NoStyle));
                    } else {
                        // Fallback to just showing the addition
                        let addition = if add.contains('\n') {
                            format!("\n{add}")
                        } else {
                            format!("`{add}`")
                        };
                        notes.push((addition.into(), Style::NoStyle));
                    }
                } else {
                    // No span provided, just show the addition
                    let addition = if add.contains('\n') {
                        format!("\n{add}")
                    } else {
                        format!("`{add}`")
                    };
                    notes.push((addition.into(), Style::NoStyle));
                }

                notes
            }
        }
    }
}