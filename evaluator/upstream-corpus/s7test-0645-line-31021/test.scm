;; Imported from upstream s7test.scm line 31021.
;; Original form:
;; (test (catch #t
;;         (lambda () (apply (list cons cons) (catch #t (lambda () (+ 1 #\a)) (lambda (type info) (list 1 2 3)))))
;;         (lambda (y i) y))
;;   '(2 . 3))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t
        (lambda () (apply (list cons cons) (catch #t (lambda () (+ 1 #\a)) (lambda (type info) (list 1 2 3)))))
        (lambda (y i) y)))))
       (expected (upstream-safe (lambda () '(2 . 3))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31021 actual expected ok?))
