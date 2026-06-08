(define-macro* (with-pair (a 'left) (b 'right) :rest body)
  `(let ((pair (cons ,a ,b)))
     ,@body))

(list
  (with-pair :a 'left :b 'right pair)
  (with-pair :a 'x :b 'y (car pair))
  (with-pair 'm 'n (cdr pair)))
