(sort! (vector 3 1 2) (lambda (a b) (if (< a b) (values) #f)))
