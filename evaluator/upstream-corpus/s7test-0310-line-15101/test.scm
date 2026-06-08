;; Imported from upstream s7test.scm line 15101.
;; Original form:
;; (test (let ((lst (cons (cons 1 2) 3)))
;; 	(set-car! (car lst) lst)
;; 	(string=? (object->string lst) "#1=((#1# . 2) . 3)"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (cons (cons 1 2) 3)))
	(set-car! (car lst) lst)
	(string=? (object->string lst) "#1=((#1# . 2) . 3)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15101 actual expected ok?))
