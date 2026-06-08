;; Imported from upstream s7test.scm line 25030.
;; Original form:
;; (test (let () (define (func args) args) (format #f "~W" func)) "(lambda (args) args)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (func args) args) (format #f "~W" func)))))
       (expected (upstream-safe (lambda () "(lambda (args) args)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25030 actual expected ok?))
