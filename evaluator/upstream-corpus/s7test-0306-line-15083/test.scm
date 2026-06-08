;; Imported from upstream s7test.scm line 15083.
;; Original form:
;; (test (let ((lst (list 1)))
;; 	(set! (car lst) lst)
;; 	(set! (cdr lst) lst)
;; 	(string=? (object->string lst) "#1=(#1# . #1#)"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1)))
	(set! (car lst) lst)
	(set! (cdr lst) lst)
	(string=? (object->string lst) "#1=(#1# . #1#)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15083 actual expected ok?))
