(define (emit-record p id owner value tags)
  (write `(record (id ,id) (owner ,owner) (value ,value) (tags ,@tags)) p)
  (newline p))

(define (consume-one text)
  (let ((p (open-input-string text)))
    (let ((x (read p)))
      (+ (length x) (length (object->string x))))))

(let loop ((i 0) (acc 0))
  (if (= i 2600)
      acc
      (let ((p (open-output-string)))
        (emit-record p i (if (= (modulo i 4) 0) 'admin 'user) (* i 3)
                     (list 'alpha (modulo i 11) 'beta))
        (let ((text (get-output-string p)))
          (loop (+ i 1) (+ acc (consume-one text)))))))
