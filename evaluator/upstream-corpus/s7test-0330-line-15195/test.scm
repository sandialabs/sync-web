;; Imported from upstream s7test.scm line 15195.
;; Original form:
;; (test (equal? (list "hi" "hi" "hi") '("hi" "hi" "hi")) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (equal? (list "hi" "hi" "hi") '("hi" "hi" "hi")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15195 actual expected ok?))
