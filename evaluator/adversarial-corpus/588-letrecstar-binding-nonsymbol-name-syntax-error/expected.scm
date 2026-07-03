(error (syntax-error ("bad variable name ~W in ~A (it is ~A, not a symbol) in ~A" 1 letrec* "an integer" "(letrec* ((x 1) (1 2)) x)")))
