(error (syntax-error ("let* variable declaration has more than one value?: ~A in ~A" (x 1 . 2) "(let* ((x 1 . 2)) x)")))
