;; Imported from upstream s7test.scm line 31275.
;; Original form:
;; (test (let () (define (try and!) (and! #f (error 'test-error "oops"))) (try and)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (try and!) (and! #f (error 'test-error "oops"))) (try and)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31275 actual expected ok?))
