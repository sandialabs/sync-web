;; Imported from upstream s7test.scm line 1724.
;; Original form:
;; (test (eq? '; a comment
;;          hi 'hi) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? '; a comment
         hi 'hi))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1724 actual expected ok?))
