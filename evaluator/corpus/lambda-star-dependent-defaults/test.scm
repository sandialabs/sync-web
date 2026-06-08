(define* (foo (a 0) (b (+ a 4)) (c (+ a 7)))
  (list a b c))

(list
  (foo)
  (foo :b 2 :a 60)
  (foo 1)
  (foo 1 2))
