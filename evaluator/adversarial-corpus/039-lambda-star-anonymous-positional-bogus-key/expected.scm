(error (wrong-type-arg ("~A: unknown key: ~S in ~S" ((lambda* ((a 1) (b 2)) a) 1 :bogus 2) (:bogus 2) (1 :bogus 2))))
