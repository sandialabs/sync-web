(error (syntax-error ("bad variable name ~W in let (it is ~A, not a symbol) in ~A" 1 "an integer" "(let loop ((1 2)) 3)")))
