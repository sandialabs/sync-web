(define* (f a (b 20) (c 30))
  (list a b c))

(list
  (list 'positional (f 1 2 3))
  (list 'defaulted (f 1))
  (list 'keywords (f :c 300 :a 100 :b 200))
  (list 'mixed (f 1 :c 9)))
