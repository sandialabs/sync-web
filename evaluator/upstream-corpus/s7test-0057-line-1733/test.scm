;; Imported from upstream s7test.scm line 1733.
;; Original form:
;; (test (eq? '() ;a comment
;; 	   '()) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? '() ;a comment
	   '()))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1733 actual expected ok?))
