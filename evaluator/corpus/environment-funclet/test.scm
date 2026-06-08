(define (make-adder x)
  (lambda (y) (+ x y)))

(let* ((f (make-adder 10))
       (e (funclet f)))
  (list
    (f 5)
    (let-ref e 'x)
    (begin (let-set! e 'x 20) (f 5))))
