/**
 * Tree-sitter grammar for the VB6-style BASIC that OxidizedBASIC runs.
 *
 * It covers what the IDE edits and highlights: procedures, declarations,
 * the control-flow statements, expressions with member access and calls,
 * and both comment forms (apostrophe and Rem). Keywords are case-insensitive.
 * Where the language is wider than this, the parser recovers, so one unusual
 * line doesn't stop the rest of the file from being highlighted.
 */

const PREC = {
  or: 1,
  and: 2,
  not: 3,
  compare: 4,
  concat: 5,
  add: 6,
  multiply: 7,
  unary: 8,
  power: 9,
  member: 10,
};

// Case-insensitive keyword: VB keywords are not case-sensitive.
function kw(word) {
  const pattern = word
    .split('')
    .map((c) => (/[a-z]/i.test(c) ? `[${c.toLowerCase()}${c.toUpperCase()}]` : c))
    .join('');
  return alias(new RegExp(pattern), word);
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

module.exports = grammar({
  name: 'vb',

  extras: ($) => [/[ \t\r]/, $.line_continuation],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat($._line),

    // One line of code: its statements (if any), then the end of the line.
    _line: ($) => seq(optional($._statement_list), $._eol),

    _eol: ($) => /\r?\n/,

    line_continuation: ($) => token(seq('_', /\r?\n/)),

    // An apostrophe, or Rem as a word of its own (not the start of an identifier
    // such as Remainder). Tree-sitter has no \b, so Rem must end with a
    // non-identifier character, which the comment body itself then includes.
    comment: ($) =>
      token(
        choice(
          seq("'", /[^\n]*/),
          seq(/[Rr][Ee][Mm]/, /([^A-Za-z0-9_\n][^\n]*)?/),
        ),
      ),


    _statement_list: ($) => prec.left(seq($._statement, repeat(seq(':', $._statement)))),


    _statement: ($) =>
      choice(
        $.block_word,
        $.procedure_definition,
        $.declaration,
        $.if_statement,
        $.select_statement,
        $.for_statement,
        $.do_statement,
        $.while_statement,
        $.with_statement,
        $.type_declaration,
        $.enum_declaration,
        $.def_type_statement,
        $.on_error_statement,
        $.exit_statement,
        $.return_statement,
        $.gosub_statement,
        $.stop_statement,
        $.goto_statement,
        $.call_statement,
        $.file_open_statement,
        $.file_close_statement,
        $.file_input_statement,
        $.file_line_input_statement,
        $.file_print_statement,
        $.file_get_statement,
        $.file_put_statement,
        $.file_seek_statement,
        $.print_statement,
        $.set_statement,
        $.erase_statement,
        $.redim_statement,
        $.mid_statement,
        $.option_statement,
        $.assignment,
        $.end_statement,
        $.comment,
        $.call_or_index,
        // a method or sub called without parentheses: txtName.SetFocus,
        // Unload Me, lstFruit.AddItem fruit(0), ShowPage 2
        $.command_statement,
        $.label,
      ),


    // A command: a name (or member) with arguments and no parentheses. A block
    // keyword is not a command name, so a bare `Else` or `End` on its own line
    // stays the keyword.
    command_statement: ($) =>
      prec.left(
        seq(
          field('command', choice($.identifier, $.member_access)),
          optional(commaSep1(choice($.argument_group, $.expression))),
        ),
      ),

    // `Else` on a line of its own: the continuation word of an `If` block,
    // kept as a keyword rather than read as a call to a sub named Else. The
    // other block words (`Next`, `Loop`, `Wend`, `End`, `Case`) are consumed by
    // their own blocks, so they are not standalone statements here.
    block_word: ($) => prec(5, kw('Else')),

    argument_group: ($) => prec(3, seq('(', commaSep1($.expression), ')')),






    // ---- procedures ------------------------------------------------------
    procedure_definition: ($) =>
      prec.left(
        seq(
          optional(field('scope', choice(kw('Private'), kw('Public'), kw('Static')))),
          field('kind', choice(kw('Sub'), kw('Function'))),
          field('name', $.identifier),
          optional($.parameter_list),
          optional(seq(kw('As'), field('return_type', $.type_name))),
          $._eol,
          repeat($._line),
          kw('End'),
          choice(kw('Sub'), kw('Function')),
        ),
      ),


    parameter_list: ($) =>
      seq('(', optional(seq($.parameter, repeat(seq(',', $.parameter)))), ')'),

    parameter: ($) =>
      seq(
        optional(choice(kw('ByVal'), kw('ByRef'), kw('Optional'))),
        field('name', $.identifier),
        optional(seq(kw('As'), field('type', $.type_name))),
      ),

    // ---- declarations ----------------------------------------------------
    declaration: ($) =>
      seq(
        choice(kw('Dim'), kw('Const'), kw('Global'), kw('Public'), kw('Private'), kw('Static')),
        commaSep1($.declared_variable),
      ),

    declared_variable: ($) =>
      seq(
        field('name', $.identifier),
        optional(seq('(', optional(commaSep1($.expression)), ')')),
        optional(seq(kw('As'), field('type', $.type_name))),
        optional(seq('=', field('value', $.expression))),
      ),

    // A type after As: a built-in type (Integer, String...) or a class name.
    // The built-in types take precedence over a plain identifier.
    type_name: ($) =>
      seq(
        choice(
          prec(2, $.builtin_type),
          $.identifier,
        ),
        // a fixed-length string's size: String * 10
        optional(seq('*', $.number)),
      ),

    builtin_type: ($) =>
      choice(
        kw('Integer'), kw('Long'), kw('Single'), kw('Double'), kw('Currency'),
        kw('String'), kw('Boolean'), kw('Byte'), kw('Variant'), kw('Date'),
        kw('Object'),
      ),




    // ---- control flow ----------------------------------------------------
    if_statement: ($) =>
      prec.left(
        seq(
          kw('If'),
          field('condition', $.expression),
          kw('Then'),
          choice(
            // single-line form: If x Then a = 1 Else a = 2
            seq($._statement_list, optional(seq(kw('Else'), $._statement_list))),
            // block form
            seq(
              $._eol,
              repeat($._line),
              repeat($.else_if_clause),
              optional($.else_clause),
              kw('End'),
              kw('If'),
            ),
          ),
        ),
      ),


    else_if_clause: ($) =>
      prec.left(seq(kw('ElseIf'), $.expression, kw('Then'), $._eol, repeat($._line))),

    else_clause: ($) => prec.left(seq(kw('Else'), $._eol, repeat($._line))),


    select_statement: ($) =>
      seq(
        kw('Select'),
        kw('Case'),
        field('subject', $.expression),
        $._eol,
        repeat($.case_clause),
        kw('End'),
        kw('Select'),
      ),

    case_clause: ($) =>
      prec.left(
        seq(kw('Case'), choice(kw('Else'), commaSep1($.case_item)), $._eol, repeat($._line)),
      ),

    // one item of a Case: a value, a range `1 To 5`, or a comparison `Is > 9`
    case_item: ($) =>
      choice(
        seq(kw('Is'), $.comparison_op, $.expression),
        seq($.expression, kw('To'), $.expression),
        $.expression,
      ),

    comparison_op: ($) => choice('<=', '>=', '<>', '<', '>', '='),


    for_statement: ($) =>
      prec.left(
        seq(
          kw('For'),
          optional(kw('Each')),
          field('variable', $.identifier),
          choice(
            seq('=', $.expression, kw('To'), $.expression, optional(seq(kw('Step'), $.expression))),
            seq(kw('In'), $.expression),
          ),
          $._eol,
          repeat($._line),
          kw('Next'),
          optional($.identifier),
        ),
      ),

    do_statement: ($) =>
      prec.left(
        seq(
          kw('Do'),
          optional(choice(kw('While'), kw('Until'))),
          optional($.expression),
          $._eol,
          repeat($._line),
          kw('Loop'),
          optional(choice(kw('While'), kw('Until'))),
          optional($.expression),
        ),
      ),

    // With obj ... End With: inside, a leading .member is a member of obj
    with_statement: ($) =>
      prec.left(seq(kw('With'), $.expression, $._eol, repeat($._line), kw('End'), kw('With'))),

    // Type Name ... End Type: a record, one field per line
    type_declaration: ($) =>
      seq(kw('Type'), $.identifier, $._eol, repeat($.type_field), kw('End'), kw('Type')),
    type_field: ($) =>
      seq(
        $.identifier,
        optional(seq('(', optional(commaSep1($.expression)), ')')),
        kw('As'),
        $.type_name,
        $._eol,
      ),

    // Enum Name ... End Enum: named constants, each optionally given a value
    enum_declaration: ($) =>
      seq(kw('Enum'), $.identifier, $._eol, repeat($.enum_member), kw('End'), kw('Enum')),
    enum_member: ($) => seq($.identifier, optional(seq('=', $.expression)), $._eol),

    // DefInt A-Z, s-t: undeclared names starting with those letters get the type
    def_type_statement: ($) =>
      seq(
        choice(
          kw('DefInt'),
          kw('DefLng'),
          kw('DefStr'),
          kw('DefBool'),
          kw('DefDbl'),
          kw('DefSng'),
          kw('DefCur'),
          kw('DefDate'),
          kw('DefVar'),
          kw('DefObj'),
          kw('DefVrnt'),
        ),
        commaSep1(seq($.identifier, optional(seq('-', $.identifier)))),
      ),

    while_statement: ($) =>
      prec.left(seq(kw('While'), $.expression, $._eol, repeat($._line), kw('Wend'))),


    on_error_statement: ($) =>
      seq(
        kw('On'),
        kw('Error'),
        choice(seq(kw('Resume'), kw('Next')), seq(kw('GoTo'), choice($.number, $.identifier))),
      ),

    exit_statement: ($) => seq(kw('Exit'), choice(kw('Sub'), kw('Function'), kw('For'), kw('Do'))),

    return_statement: ($) => seq(kw('Return'), optional($.expression)),

    gosub_statement: ($) => seq(kw('GoSub'), choice($.number, $.identifier)),

    stop_statement: ($) => kw('Stop'),

    goto_statement: ($) => seq(kw('GoTo'), choice($.number, $.identifier)),

    end_statement: ($) => kw('End'),

    // ---- simple statements -----------------------------------------------
    call_statement: ($) => seq(kw('Call'), $.expression),

    // File I/O: Open, Close, Input #, Line Input #, Print #, Write #
    file_open_statement: ($) =>
      seq(
        kw('Open'),
        $.expression,
        kw('For'),
        choice(kw('Input'), kw('Output'), kw('Append'), kw('Random'), kw('Binary')),
        kw('As'),
        optional('#'),
        $.expression,
        optional(seq(kw('Len'), '=', $.expression)),
      ),

    // Get #n, [recnum], var and Put #n, [recnum], value
    file_get_statement: ($) =>
      seq(kw('Get'), optional('#'), $.expression, ',', optional($.expression), ',', $.expression),

    file_put_statement: ($) =>
      seq(kw('Put'), optional('#'), $.expression, ',', optional($.expression), ',', $.expression),

    // Seek #n, pos
    file_seek_statement: ($) =>
      seq(kw('Seek'), optional('#'), $.expression, ',', $.expression),

    file_close_statement: ($) =>
      seq(kw('Close'), optional(commaSep1(seq(optional('#'), $.expression)))),

    file_input_statement: ($) =>
      seq(kw('Input'), '#', $.expression, ',', commaSep1($.expression)),

    file_line_input_statement: ($) =>
      seq(kw('Line'), kw('Input'), '#', $.expression, ',', $.expression),

    file_print_statement: ($) =>
      seq(
        choice(kw('Print'), kw('Write')),
        '#',
        $.expression,
        optional(
          seq(',', $.expression, repeat(seq(choice(';', ','), optional($.expression)))),
        ),
      ),

    print_statement: ($) =>
      seq(
        choice(seq(kw('Debug'), '.', kw('Print')), kw('Print')),
        optional(
          choice(
            // one or more items, each separated by ; or , (a trailing ; is allowed)
            seq($.expression, repeat(seq(choice(';', ','), optional($.expression)))),
            // a line of only separators, such as "Print ;" or "Print ,"
            seq(choice(';', ','), repeat(seq(optional($.expression), choice(';', ',')))),
          ),
        ),
      ),


    // a line label such as Oops: (a GoTo or On Error target); precedence keeps it
    // from being read as a command named Oops
    label: ($) => prec(2, seq($.identifier, ':')),



    set_statement: ($) => seq(kw('Set'), $._target, '=', $.expression),

    // Let is an optional keyword on an assignment
    assignment: ($) => prec(1, seq(optional(kw('Let')), $._target, '=', $.expression)),

    // Erase a, b: resets arrays
    erase_statement: ($) => seq(kw('Erase'), commaSep1($._target)),

    // ReDim [Preserve] a(n), b(n): resizes dynamic arrays
    redim_statement: ($) =>
      seq(
        kw('ReDim'),
        optional(kw('Preserve')),
        commaSep1(seq(field('name', $.identifier), $.array_dims)),
      ),

    array_dims: ($) => seq('(', optional(commaSep1($.expression)), ')'),

    // Mid(s, start [, length]) = text
    mid_statement: ($) =>
      seq(
        kw('Mid'),
        '(',
        $.expression,
        ',',
        $.expression,
        optional(seq(',', $.expression)),
        ')',
        '=',
        $.expression,
      ),

    // Option Explicit | Option Base 0|1 | Option Compare Text|Binary
    option_statement: ($) =>
      seq(
        kw('Option'),
        choice(
          kw('Explicit'),
          seq(kw('Base'), $.number),
          seq(kw('Compare'), choice(kw('Text'), kw('Binary'))),
        ),
      ),


    // what a value is stored into: a name, a member, or an indexed item
    _target: ($) => choice($.identifier, $.member_access, $.with_member, $.call_or_index),

    // .Caption inside a With block: a member of the With object
    with_member: ($) => seq('.', field('member', $.identifier)),

    // ---- expressions -----------------------------------------------------
    expression: ($) =>
      choice(
        $.binary_expression,
        $.unary_expression,
        $.member_access,
        $.with_member,
        $.call_or_index,
        $.array_literal,
        $.literal,
        $.identifier,
        seq('(', $.expression, ')'),
      ),

    binary_expression: ($) =>
      choice(
        ...[
          [PREC.or, choice(kw('Or'), kw('Xor'), kw('Eqv'), kw('Imp'))],
          [PREC.and, kw('And')],
          [PREC.compare, choice('=', '<>', '<', '<=', '>', '>=', kw('Is'), kw('Like'))],
          [PREC.concat, '&'],
          [PREC.add, choice('+', '-')],
          [PREC.multiply, choice('*', '/', '\\', kw('Mod'))],
          [PREC.power, '^'],
        ].map(([p, op]) =>
          prec.left(
            p,
            seq(field('left', $.expression), field('operator', op), field('right', $.expression)),
          ),
        ),
      ),

    unary_expression: ($) =>
      choice(
        prec(PREC.not, seq(kw('Not'), $.expression)),
        prec(PREC.unary, seq(choice('-', '+'), $.expression)),
      ),

    member_access: ($) =>
      prec.left(
        PREC.member,
        seq(
          field('object', choice($.identifier, $.call_or_index, $.member_access)),
          '.',
          field('member', $.identifier),
        ),
      ),

    call_or_index: ($) =>
      prec.left(
        PREC.member,
        seq(
          field('callee', choice($.identifier, $.member_access)),
          '(',
          optional(commaSep1($.expression)),
          ')',
        ),
      ),

    array_literal: ($) => seq(kw('Array'), '(', optional(commaSep1($.expression)), ')'),

    literal: ($) =>
      choice(
        $.string,
        $.number,
        $.date_literal,
        kw('True'),
        kw('False'),
        kw('Nothing'),
        kw('Null'),
        kw('Empty'),
      ),

    string: ($) => token(seq('"', repeat(choice(/[^"\n]/, '""')), '"')),

    // &HFF and &O17 are numbers; they take precedence over the & operator,
    // which is only ever followed by a space or an operand
    number: ($) =>
      choice(
        token(prec(2, choice(/&[Hh][0-9A-Fa-f]+/, /&[Oo][0-7]+/))),
        token(/(\d+\.?\d*|\.\d+)([eEdD][+-]?\d+)?[%&!#@]?/),
      ),


    date_literal: ($) => token(seq('#', /[^#\n]+/, '#')),

    identifier: ($) => /[A-Za-z_][A-Za-z0-9_]*[%&!#@$]?/,
  },
});

