((lambda* (:rest r (a (length r)) (b (+ a (length r)))) (list a b r)) 1 2)
