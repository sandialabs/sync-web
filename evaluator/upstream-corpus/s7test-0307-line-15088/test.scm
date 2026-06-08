;; Imported from upstream s7test.scm line 15088.
;; Original form:
;; (test (let ((lst (list 1)))
;; 	(set! (car lst) lst)
;; 	(set! (cdr lst) lst)
;; 	(equal? (car lst) (cdr lst)))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1)))
	(set! (car lst) lst)
	(set! (cdr lst) lst)
	(equal? (car lst) (cdr lst))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15088 actual expected ok?))
