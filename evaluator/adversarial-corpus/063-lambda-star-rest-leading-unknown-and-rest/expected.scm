(error (wrong-type-arg ("~A: unknown key: ~S in ~S" ((lambda* ((a 1) :rest r) (list a r)) :bogus 3 4 5) (:bogus 3 4 5) (:bogus 3 4 5))))
