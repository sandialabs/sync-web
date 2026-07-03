(error (wrong-type-arg ("~A: keyword argument's value is missing: ~S in ~S" ((lambda* ((a 1) (b 2)) (list a b)) :b) (:b) (:b))))
