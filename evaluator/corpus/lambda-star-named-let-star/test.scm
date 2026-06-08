(list
  (let* loop ((i 0) (j 0))
    (if (> i 3)
        (+ i j)
        (loop :j 2 :i (+ i 1))))
  (let* fib ((n 6) (a 0) (b 1))
    (if (= n 0) a (fib (- n 1) b (+ a b)))))
