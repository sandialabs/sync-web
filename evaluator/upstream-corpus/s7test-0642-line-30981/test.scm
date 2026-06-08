;; Imported from upstream s7test.scm line 30981.
;; Original form:
;; (test (let ((f1 (lambda (x) "we're number 1"))) (f1 (let () (set! f1 (lambda (x) "we're number 2"))))) "we're number 1")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((f1 (lambda (x) "we're number 1"))) (f1 (let () (set! f1 (lambda (x) "we're number 2"))))))))
       (expected (upstream-safe (lambda () "we're number 1")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30981 actual expected ok?))
