;; Imported from upstream s7test.scm line 10174.
;; Original form:
;; (test (catch #t
;;        (lambda () (let ((L1 (list 1))) (list-set! L1 3 0)))
;;        (lambda (type info) (apply format #f info)))
;;       "list-set! second argument, 3, is out of range (it is too large)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t
       (lambda () (let ((L1 (list 1))) (list-set! L1 3 0)))
       (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "list-set! second argument, 3, is out of range (it is too large)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10174 actual expected ok?))
