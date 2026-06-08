(sort! #r(2.0 4.0 1.0 3.0) (lambda (a b) (if (< a b) (values) #f)))
