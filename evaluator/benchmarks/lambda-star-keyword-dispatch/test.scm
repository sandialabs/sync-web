(define* (normalize (path (error 'arg-error "missing path")) (value #f) (metadata '()) (expression? #f) (replace? #t))
  (let ((base (+ (length path) (if value 3 0) (length metadata))))
    (+ base (if expression? 5 0) (if replace? 7 11))))

(let loop ((i 0) (acc 0))
  (if (= i 70000)
      acc
      (loop (+ i 1)
            (+ acc
               (if (= (remainder i 3) 0)
                   (normalize (list 'users (remainder i 29) 'doc) :value i :expression? #t)
                   (normalize :metadata (list (cons 'rev i)) :path (list 'public 'item (remainder i 17)) :replace? #f))))))
