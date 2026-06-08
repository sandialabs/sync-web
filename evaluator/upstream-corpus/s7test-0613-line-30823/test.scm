;; Imported from upstream s7test.scm line 30823.
;; Original form:
;; (test (catch #t (lambda () (let ((x 0)) (define-macro (hi) 'x) (set! (hi) 3) x)) (lambda (typ info) (apply format #f info)))
;;       "hi (a macro) does not have a setter: (set! (hi) 3)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((x 0)) (define-macro (hi) 'x) (set! (hi) 3) x)) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "hi (a macro) does not have a setter: (set! (hi) 3)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30823 actual expected ok?))
