;; Imported from upstream s7test.scm line 16701.
;; Original form:
;; (test (catch #t (lambda () (make-hash-table 8 eq? #t)) (lambda (type info) (apply format #f info)))
;;         "make-hash-table third argument, #t, is boolean but should be either #f or (cons key-type-check value-type-check)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (make-hash-table 8 eq? #t)) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "make-hash-table third argument, #t, is boolean but should be either #f or (cons key-type-check value-type-check)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16701 actual expected ok?))
