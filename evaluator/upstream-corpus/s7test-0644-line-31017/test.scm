;; Imported from upstream s7test.scm line 31017.
;; Original form:
;; (test (catch #t  ; t_c_function branch in implicit_index
;;         (lambda () (apply (list cons cons) (catch #t (lambda () (+ 1 #\a)) (lambda (type info) (list 1 2)))))
;;         (lambda (y i) y))
;;   'wrong-number-of-args)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t  ; t_c_function branch in implicit_index
        (lambda () (apply (list cons cons) (catch #t (lambda () (+ 1 #\a)) (lambda (type info) (list 1 2)))))
        (lambda (y i) y)))))
       (expected (upstream-safe (lambda () 'wrong-number-of-args)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31017 actual expected ok?))
