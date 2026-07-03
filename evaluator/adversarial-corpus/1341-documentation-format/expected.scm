"(format out str . args) substitutes args into str sending the result to out. Most of s7's format directives are taken from CL: ~% = newline, ~& = newline if the preceding output character was no a newline, ~~ = ~, ~<newline> trims white space, ~* skips an argument, ~^ exits {} iteration if the arg list is exhausted, ~nT spaces over to column n, ~A prints a representation of any object, ~S is the same, but puts strings in double quotes, ~C prints a character, numbers are handled by ~F, ~E, ~G, ~B, ~O, ~D, and ~X with preceding numbers giving spacing (and spacing character) and precision.  ~{ starts an embedded format directive which is ended by ~}: 

  >(format #f \"dashed: ~{~A~^-~}\" '(1 2 3))
  \"dashed: 1-2-3\"

~P inserts \"s\" if the current it is not 1 or 1.0 (use ~@P for \"ies\" or \"y\").
~B is number->string in base 2, ~O in base 8, ~D base 10, ~X base 16,
~E: (format #f \"~E\" 100.1) -&gt; \"1.001000e+02\" (%e in C)
~F: (format #f \"~F\" 100.1) -&gt; \"100.100000\"   (%f in C)
~G: (format #f \"~G\" 100.1) -&gt; \"100.1\"        (%g in C)

If the 'out' it is not an output port, the resultant string is returned.  If it is #t, the string is also sent to the current-output-port."
