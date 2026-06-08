(error (syntax-error ("macroexpand argument is not a macro call: ~A" ('((macro (x y) (list-values '+ x y)) 2 3)))))
