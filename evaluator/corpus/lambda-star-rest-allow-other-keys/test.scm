(define* (collect a :rest rest)
  (list a rest))

(define* (wrapper a :rest rest :allow-other-keys)
  (list a rest))

(list
  (list 'plain (collect 1 2 3 4))
  (list 'keyword-as-rest (collect :a 1))
  (list 'allow-other (wrapper :unknown 9 :a 3)))
