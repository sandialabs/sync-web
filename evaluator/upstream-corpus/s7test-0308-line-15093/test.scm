;; Imported from upstream s7test.scm line 15093.
;; Original form:
;; (test (let ((lst (cons 1 2)))
;; 	(set-cdr! lst lst)
;; 	(string=? (object->string lst) "#1=(1 . #1#)"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (cons 1 2)))
	(set-cdr! lst lst)
	(string=? (object->string lst) "#1=(1 . #1#)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15093 actual expected ok?))
