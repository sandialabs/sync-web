;; Imported from upstream s7test.scm line 25031.
;; Original form:
;; (test (let () (define func (lambda (a . b) a)) (format #f "~W" func)) "(lambda (a . b) a)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define func (lambda (a . b) a)) (format #f "~W" func)))))
       (expected (upstream-safe (lambda () "(lambda (a . b) a)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25031 actual expected ok?))
