(error (syntax-error ("macroexpand argument is not a macro call: ~A" ('((macro (x) (list-values '+ x 1)) 2)))))
