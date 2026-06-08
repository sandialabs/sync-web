((lambda* (:rest r (a (begin (set! r (cons 0 r)) (length r)))) (list a r)) 1 2)
