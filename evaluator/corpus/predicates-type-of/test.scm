(list
  (map type-of (list #t #f '() 1 1.5 1/2 1+i "s" 'sym :key '(1 . 2) #(1) #u(1)))
  (list (boolean? #f) (null? '()) (number? 1/2) (string? "x") (symbol? 'x) (keyword? :x))
  (list (pair? '(1 . 2)) (list? '(1 2)) (vector? #(1)) (byte-vector? #u(1))))
