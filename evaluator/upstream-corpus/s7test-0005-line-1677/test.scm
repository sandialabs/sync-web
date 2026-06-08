;; Imported from upstream s7test.scm line 1677.
;; Original form:
;; (test (eq? "hi" "hi") #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? "hi" "hi"))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1677 actual expected ok?))
