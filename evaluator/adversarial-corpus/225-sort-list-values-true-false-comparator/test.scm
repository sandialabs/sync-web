(sort! (list 3 1 2) (lambda (a b) (if (< a b) (values #t #f) #f)))
