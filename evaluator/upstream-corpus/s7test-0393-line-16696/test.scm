;; Imported from upstream s7test.scm line 16696.
;; Original form:
;; (test (catch #t (lambda () (set! (:asdf 3) 2)) (lambda (type info) (apply format #f info)))
;;       "in (set! (:asdf 3) 2), :asdf has no setter")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (set! (:asdf 3) 2)) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "in (set! (:asdf 3) 2), :asdf has no setter")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16696 actual expected ok?))
