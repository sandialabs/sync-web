(define* (f a (b 2)) (list a b))
(define* (g a :rest rest) (list a rest))
(define* (h a :allow-other-keys) a)

(list
  (catch #t (lambda () (f)) (lambda args args))
  (catch #t (lambda () (f 1 :unknown 2)) (lambda args args))
  (catch #t (lambda () (f :b 3 :b 4 :a 1)) (lambda args args))
  (g 1 :unknown 2)
  (h :unknown 9 :a 3))
