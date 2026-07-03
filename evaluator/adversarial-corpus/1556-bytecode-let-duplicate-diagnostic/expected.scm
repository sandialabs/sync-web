(error (syntax-error ("duplicate identifier in let: ~S in ~S" x (let ((x 1) (x 2)) x))))
