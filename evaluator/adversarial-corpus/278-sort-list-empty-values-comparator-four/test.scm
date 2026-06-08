(sort! (list 2 4 1 3) (lambda (a b) (if (< a b) (values) #f)))
