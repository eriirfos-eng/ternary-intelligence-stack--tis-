/**
 * tree-sitter-ternlang — Tree-sitter grammar for the Ternlang language
 *
 * Ternlang is a balanced ternary systems programming language by RFI-IRFOS.
 * Canonical specification: https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-
 * BET VM spec: BET-ISA-SPEC.md
 *
 * Trit semantics:
 *   -1 (reject)  — definitively negative / conflict
 *    0 (tend)    — active hold; not null, not zero, computationally meaningful
 *   +1 (affirm)  — definitively positive / truth
 *
 * License: LGPL-3.0-or-later
 */

module.exports = grammar({
  name: "ternlang",

  extras: ($) => [$.comment, /\s/],

  rules: {
    // ─── Top-level ────────────────────────────────────────────────────────────
    source_file: ($) =>
      repeat(
        choice(
          $.function_definition,
          $.agent_definition,
          $.struct_definition,
          $.use_declaration,
          $.statement,
        ),
      ),

    // ─── Comments ─────────────────────────────────────────────────────────────
    comment: (_) => token(seq("//", /.*/)),

    // ─── Directives ───────────────────────────────────────────────────────────
    // @sparseskip: the flagship BET annotation — skips zero-trit elements in
    // tensor loops, reducing multiply-ops by the sparsity fraction.
    directive: ($) =>
      seq("@", choice("sparseskip", "inline", "extern", "borrow"), optional($.identifier)),

    // ─── Use declarations ─────────────────────────────────────────────────────
    use_declaration: ($) => seq("use", $.module_path, ";"),

    module_path: ($) =>
      seq($.identifier, repeat(seq("::", $.identifier))),

    // ─── Struct definition ────────────────────────────────────────────────────
    struct_definition: ($) =>
      seq(
        optional("pub"),
        "struct",
        field("name", $.identifier),
        "{",
        repeat($.struct_field),
        "}",
      ),

    struct_field: ($) =>
      seq(field("name", $.identifier), ":", field("type", $.type_expression), optional(",")),

    // ─── Agent definition (actor model) ───────────────────────────────────────
    agent_definition: ($) =>
      seq(
        "agent",
        field("name", $.identifier),
        "{",
        repeat($.function_definition),
        "}",
      ),

    // ─── Function definition ──────────────────────────────────────────────────
    function_definition: ($) =>
      seq(
        optional("pub"),
        "fn",
        field("name", $.identifier),
        "(",
        optional($.parameter_list),
        ")",
        optional(seq("->", field("return_type", $.type_expression))),
        field("body", $.block),
      ),

    parameter_list: ($) =>
      seq($.parameter, repeat(seq(",", $.parameter))),

    parameter: ($) =>
      seq(
        optional("mut"),
        field("name", $.identifier),
        ":",
        field("type", $.type_expression),
      ),

    // ─── Types ────────────────────────────────────────────────────────────────
    type_expression: ($) =>
      choice(
        "trit",
        seq("trittensor", "<", $.integer_literal, "x", $.integer_literal, ">"),
        "i64",
        "f64",
        "bool",
        "string",
        "agentref",
        $.identifier,
      ),

    // ─── Block ────────────────────────────────────────────────────────────────
    block: ($) => seq("{", repeat($.statement), "}"),

    // ─── Statements ───────────────────────────────────────────────────────────
    statement: ($) =>
      choice(
        $.let_statement,
        $.assignment_statement,
        $.field_assignment_statement,
        $.return_statement,
        $.if_statement,
        $.match_statement,
        $.for_statement,
        $.while_statement,
        $.loop_statement,
        $.break_statement,
        $.continue_statement,
        $.spawn_statement,
        $.send_statement,
        $.expression_statement,
        $.directive,
      ),

    let_statement: ($) =>
      seq(
        "let",
        optional("mut"),
        field("name", $.identifier),
        optional(seq(":", field("type", $.type_expression))),
        optional(seq("=", field("value", $.expression))),
        ";",
      ),

    assignment_statement: ($) =>
      seq(field("target", $.identifier), "=", field("value", $.expression), ";"),

    field_assignment_statement: ($) =>
      seq(
        field("object", $.identifier),
        ".",
        field("field", $.identifier),
        "=",
        field("value", $.expression),
        ";",
      ),

    return_statement: ($) =>
      seq("return", optional($.expression), ";"),

    // ─── Control flow ─────────────────────────────────────────────────────────
    if_statement: ($) =>
      seq(
        "if",
        field("condition", $.expression),
        optional("?"),            // uncertain-branch operator
        field("consequence", $.block),
        optional(seq("else", field("alternative", choice($.block, $.if_statement)))),
      ),

    // match is ALWAYS 3-way exhaustive: -1, 0, +1
    match_statement: ($) =>
      seq(
        "match",
        field("subject", $.expression),
        "{",
        repeat($.match_arm),
        "}",
      ),

    match_arm: ($) =>
      seq(field("pattern", $.match_pattern), "=>", field("body", $.block)),

    // The three canonical trit patterns — Linguist should treat these as
    // first-class syntax, not just integer literals.
    match_pattern: ($) =>
      choice(
        $.trit_literal,   // -1 | 0 | 1
        $.trit_keyword,   // reject | tend | affirm
        $.identifier,
        "_",
      ),

    for_statement: ($) =>
      seq("for", $.identifier, "in", $.expression, field("body", $.block)),

    while_statement: ($) =>
      seq("while", $.expression, field("body", $.block)),

    loop_statement: ($) =>
      seq("loop", field("body", $.block)),

    break_statement: (_) => seq("break", ";"),
    continue_statement: (_) => seq("continue", ";"),

    // ─── Actor model ──────────────────────────────────────────────────────────
    spawn_statement: ($) =>
      seq(
        "let",
        $.identifier,
        "=",
        "spawn",
        optional(seq("remote", $.string_literal)),
        field("agent", $.identifier),
        ";",
      ),

    send_statement: ($) =>
      seq("send", field("agent", $.expression), field("message", $.expression), ";"),

    // ─── Expressions ──────────────────────────────────────────────────────────
    expression_statement: ($) => seq($.expression, ";"),

    expression: ($) =>
      choice(
        $.binary_expression,
        $.unary_expression,
        $.call_expression,
        $.field_access,
        $.await_expression,
        $.cast_expression,
        $.trit_literal,
        $.trit_keyword,
        $.integer_literal,
        $.float_literal,
        $.string_literal,
        $.bool_literal,
        $.identifier,
      ),

    binary_expression: ($) =>
      prec.left(
        1,
        seq(
          field("left", $.expression),
          field("operator", choice("+", "-", "*", "==", "!=", "&&", "||")),
          field("right", $.expression),
        ),
      ),

    unary_expression: ($) =>
      prec(2, seq(field("operator", choice("-", "!")), field("operand", $.expression))),

    call_expression: ($) =>
      seq(
        field("function", choice($.identifier, $.module_path)),
        "(",
        optional($.argument_list),
        ")",
      ),

    argument_list: ($) => seq($.expression, repeat(seq(",", $.expression))),

    field_access: ($) =>
      seq(field("object", $.identifier), ".", field("field", $.identifier)),

    await_expression: ($) =>
      seq("await", field("agent", $.expression)),

    cast_expression: ($) =>
      seq("cast", "(", $.expression, ")"),

    // ─── Trit literals (THE core primitive of the language) ───────────────────
    // -1, 0, +1 are NOT integers in Ternlang; they are trit values.
    // Linguist should highlight them distinctly as constants.
    trit_literal: (_) =>
      token(choice("-1", "0", "1", "+1")),

    // Semantic trit keywords — first-class aliases for the three trit states
    trit_keyword: (_) =>
      token(choice("affirm", "tend", "reject")),

    // ─── Primitives ───────────────────────────────────────────────────────────
    integer_literal: (_) => /[0-9]+/,

    float_literal: (_) => /[0-9]+\.[0-9]+/,

    string_literal: (_) => /"[^"]*"/,

    bool_literal: (_) => choice("true", "false"),

    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,
  },
});
