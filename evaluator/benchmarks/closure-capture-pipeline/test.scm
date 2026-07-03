(define (make-stage name weight)
  (lambda (item)
    (let ((value (item 'value))
          (tags (item 'tags)))
      (set! (item 'value) (+ value weight))
      (set! (item 'tags) (cons name tags))
      item)))

(define (run-pipeline stages item)
  (let loop ((ss stages) (x item))
    (if (null? ss)
        x
        (loop (cdr ss) ((car ss) x)))))

(let ((stages (list (make-stage 'normalize 1)
                    (make-stage 'authorize 2)
                    (make-stage 'index 3)
                    (make-stage 'respond 4))))
  (let loop ((i 0) (acc 0))
    (if (= i 4300)
        acc
        (let* ((item (inlet 'id i 'value (modulo i 101) 'tags '()))
               (out (run-pipeline stages item)))
          (loop (+ i 1) (+ acc (out 'value) (length (out 'tags))))))))
