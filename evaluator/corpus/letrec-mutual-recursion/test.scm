(letrec ((evenish? (lambda (n) (if (= n 0) #t (oddish? (- n 1)))))
         (oddish? (lambda (n) (if (= n 0) #f (evenish? (- n 1))))))
  (list (evenish? 10) (oddish? 10) (evenish? 11) (oddish? 11)))
