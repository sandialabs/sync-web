;; Imported from upstream s7test.scm line 31273.
;; Original form:
;; (test (let ((and! and)) (and! #f (error 'test-error "oops"))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((and! and)) (and! #f (error 'test-error "oops"))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31273 actual expected ok?))
