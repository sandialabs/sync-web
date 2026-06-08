;; Imported from upstream s7test.scm line 20582.
;; Original form:
;; (test (let ((val #f)) (call-with-output-string (lambda (p) (set! val (output-port? p)))) val) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((val #f)) (call-with-output-string (lambda (p) (set! val (output-port? p)))) val))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20582 actual expected ok?))
