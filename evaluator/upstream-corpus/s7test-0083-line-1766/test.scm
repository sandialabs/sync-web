;; Imported from upstream s7test.scm line 1766.
;; Original form:
;; (test (let ((f (lambda () (cons 1 (string #\H))))) (eq? (f) (f))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((f (lambda () (cons 1 (string #\H))))) (eq? (f) (f))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1766 actual expected ok?))
