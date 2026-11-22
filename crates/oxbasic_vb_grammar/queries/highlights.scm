; Highlight queries for the IDE. The captures are the IDE's token kinds:
; keyword, string, number, comment, identifier and punctuation.

[
  (comment)
] @comment

(string) @string
(number) @number
(date_literal) @number

[
  "Sub" "Function" "End" "Dim" "Const" "Static" "Global" "Private" "Public"
  "ByVal" "ByRef" "Optional" "As" "If" "Then" "Else" "ElseIf" "Select" "Case"
  "For" "Each" "In" "To" "Step" "Next" "Do" "Loop" "While" "Until" "Wend"
  "Exit" "Return" "GoTo" "On" "Error" "Resume" "Call" "Set" "Print" "Debug"
  "And" "Or" "Xor" "Not" "Eqv" "Imp" "Is" "Like" "Mod"
  "True" "False" "Nothing" "Null" "Empty" "Array"
  "With" "Type" "Enum" "Stop" "Option" "Explicit" "Base" "Compare" "Let" "Erase"
  "ReDim" "Preserve" "Mid" "GoSub" "Return"
] @keyword

(builtin_type) @keyword

(procedure_definition name: (identifier) @function)
(parameter name: (identifier) @variable.parameter)
(declared_variable name: (identifier) @variable)
(member_access member: (identifier) @property)
(label (identifier) @label)
(identifier) @variable
