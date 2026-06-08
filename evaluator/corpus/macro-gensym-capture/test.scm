(define-macro (swap! a b)
  (let ((tmp (gensym)))
    `(let ((,tmp ,a))
       (set! ,a ,b)
       (set! ,b ,tmp))))

(let ((x 1) (y 2) (tmp 99))
  (swap! x y)
  (list x y tmp))
