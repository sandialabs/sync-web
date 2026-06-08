(sort! (list 4 1 3 2) (lambda (a b) (if (< a b) (values #t #f) #f)))
