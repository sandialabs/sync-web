;; Imported from upstream s7test.scm line 20583.
;; Original form:
;; (test (let ((res #f)) (let ((this-file (open-output-string))) (set! res (output-port? this-file)) (close-output-port this-file) res)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((res #f)) (let ((this-file (open-output-string))) (set! res (output-port? this-file)) (close-output-port this-file) res)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20583 actual expected ok?))
