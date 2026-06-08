;; Imported from upstream s7test.scm line 31274.
;; Original form:
;; (test (let ((and! #f)) (set! and! and) (and! #f (error 'test-error "oops"))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((and! #f)) (set! and! and) (and! #f (error 'test-error "oops"))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31274 actual expected ok?))
