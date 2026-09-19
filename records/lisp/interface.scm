(macro (config standard-module chain tree ledger federation authorization . classes)
  (define empty-state-digest
    #u(0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
       0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0))
  (define released-1.6-classes-digest
    #u(91 119 66 201 209 168 212 3 60 87 37 164 22 240 61 159
       47 246 11 249 24 136 65 37 160 35 72 105 214 166 106 219))
  (define fresh? (equal? (sync-digest *sync-state*) empty-state-digest))
  (set! chain (eval chain))
  (set! tree (eval tree))
  (set! ledger (eval ledger))
  (set! federation (eval federation))
  (set! authorization (eval authorization))
  (define (cfg key) (cadr (assoc key config)))
  (for-each (lambda (key)
              (if (not (assoc key config)) (error 'argument-error "Missing config")))
            '(root-secret interface-secret root interface name))
  (if (equal? (sync-hash (expression->byte-vector (cfg 'root-secret)))
              (sync-hash (expression->byte-vector (cfg 'interface-secret))))
      (error 'argument-error "Root and Interface credentials must differ"))
  (set! config (append config '((clear? #t) (admins ()) (window #f)
                                (bridge-accept auto))))
  (if (eq? fresh? (not (cfg 'clear?)))
      (error 'upgrade-error
             "Fresh installation and versioned upgrade state do not match clear?"))

  (define standard-source (eval standard-module))
  (define standard-module* (eval standard-source))
  (define standard-class (standard-module* 'class))
  (define (call body) (let ((result (sync-call `(*call* ,(cfg 'root-secret) ,body) #t)))
			(if (and (pair? result) (eq? (car result) 'error)) (apply error (cdr result)) result)))
  (define (set-query body) (sync-call `(*set-query* ,(cfg 'root-secret) ,body) #t))
  (define (set-step body) (sync-call `(*set-step* ,(cfg 'root-secret) ,body) #t))

  (if fresh?
      (begin
        (let ((result (sync-call `(,(cfg 'root) ,(cfg 'root-secret) fresh) #t)))
          (if (and (pair? result) (eq? (car result) 'error))
              (apply error (cdr result))))
        (call `(lambda (root) (let* ((standard-module (eval ',standard-source))
			       (standard-node (standard-module 'make))
			       (standard-class (standard-module 'class))
			       (key-derivation-salt ,(random-byte-vector 32))
			       (journal-keys (crypto-generate (expression->byte-vector
							       (list 'sync-web/journal-signing-key/v1 key-derivation-salt
								     (sync-hash (expression->byte-vector ,(cfg 'root-secret)))))))
                               (interface-keys
                                (crypto-generate
                                 (expression->byte-vector
                                  (list 'sync-web/interface-signing-key/v1 key-derivation-salt
                                        (sync-hash
                                         (expression->byte-vector ,(cfg 'interface-secret)))))))
			       (federation-node
                                (standard-module 'init ',federation
                                 (list standard-node
                                       `((endpoint ,,(cfg 'interface))
                                         (bridge-accept ,',(cfg 'bridge-accept))
                                         (public-key ,(car interface-keys))
                                         (peers ()) (retired ())))))
                               (federation-live
                                (standard-module 'local ',federation federation-node))
                               (interface-public-key (car interface-keys))
                               (ledger-config
                                `((public
                                   ((window ,,(cfg 'window))
                                    (key-derivation-salt ,key-derivation-salt)
                                    (public-key ,(car journal-keys))
                                    (journal ((public-key ,(car journal-keys))
                                              (latest-rotation-index -1)))
                                    (interface ((public-key ,interface-public-key)
                                                (endpoint ,,(cfg 'interface))))
                                    (bridge-accept ,',(cfg 'bridge-accept))
                                    (name ,',(cfg 'name))))
                                  (private
                                   ((bridge-preapproval ())
                                    (journal ((rotation-indexes ())
                                              (pending-rotation ())))))))
                               (ledger-node
                                (standard-module 'init ',ledger
                                 (list standard-node ledger-config ',tree ',chain)))
                               (authorization-node
                                (standard-module 'init ',authorization '())))
			  ((root 'set!) '(root class standard-module) ',standard-source)
			((root 'set!) '(root class standard) standard-class)
			  ((root 'set!) '(root class chain) ',chain)
			  ((root 'set!) '(root class tree) ',tree)
			  ((root 'set!) '(root class ledger) ',ledger)
			  ((root 'set!) '(root class federation) ',federation)
			  ((root 'set!) '(root class authorization) ',authorization)
			  ((root 'set!) '(root object standard) standard-node)
			  ((root 'set!) '(root object ledger) ledger-node)
			  ((root 'set!) '(root object federation) (federation-live))
			  ((root 'set!) '(root object authorization) authorization-node)
                          ((root 'set!) '(interface credential) ,(cfg 'interface-secret))
                          ((root 'set!) '(interface root-verifier)
                           (sync-hash (expression->byte-vector ,(cfg 'root-secret))))
			  ((root 'set!) '(interface secret)
			   (sync-hash (expression->byte-vector ,(cfg 'interface-secret))))
			  ((root 'set!) '(interface admins) ',(cfg 'admins))
			  ((root 'set!) '(interface endpoint) ,(cfg 'interface))
			  ((root 'set!) '(interface name) ',(cfg 'name))
			  #t))))
      (call
       `(lambda (root)
          (let* ((standard-module (eval ',standard-source))
                 (standard-node (standard-module 'make))
                 (old-ledger
                  (sync-eval ((root 'get) '(root object ledger))))
                 (old-federation
                  (sync-eval ((root 'get) '(root object federation))))
                 (ledger-config ((old-ledger 'config)))
                 (public (cadr (assoc 'public ledger-config)))
                 (private (cadr (assoc 'private ledger-config)))
                 (identity (and (assoc 'identity public)
                                (cadr (assoc 'identity public))))
                 (identity-id (and (list? identity) (assoc 'id identity)
                                   (cadr (assoc 'id identity))))
                 (identity-nonce (and (list? identity) (assoc 'nonce identity)
                                      (cadr (assoc 'nonce identity))))
                 (key-derivation-salt
                  (and (assoc 'key-derivation-salt public)
                       (cadr (assoc 'key-derivation-salt public))))
                 (key-seed (or identity-id key-derivation-salt))
                 (journal-key (cadr (assoc 'public-key public)))
                 (interface-config (cadr (assoc 'interface public)))
                 (interface-key (cadr (assoc 'public-key interface-config)))
                 (derived-journal
                  (car (crypto-generate
                        (expression->byte-vector
                         (list 'sync-web/journal-signing-key/v1 key-seed
                               (sync-hash
                                (expression->byte-vector ,(cfg 'root-secret))))))))
                 (derived-interface
                  (car (crypto-generate
                        (expression->byte-vector
                         (list 'sync-web/interface-signing-key/v1 key-seed
                               (sync-hash
                                (expression->byte-vector ,(cfg 'interface-secret))))))))
                 (federation-config ((old-federation 'config)))
                 (released-1.6?
                  (and (not identity)
                       (byte-vector? key-derivation-salt)
                       (= (length key-derivation-salt) 32)
                       (equal? journal-key derived-journal)
                       (equal? interface-key derived-interface)
                       (equal? interface-key
                               (cadr (assoc 'public-key federation-config)))
                       (equal?
                        (sync-hash
                         (expression->byte-vector
                          (list ((root 'get) '(root class standard-module))
                                ((root 'get) '(root class standard))
                                ((root 'get) '(root class chain))
                                ((root 'get) '(root class tree))
                                ((root 'get) '(root class ledger))
                                ((root 'get) '(root class federation))
                                ((root 'get) '(root class authorization)))))
                        ',released-1.6-classes-digest))))
            (define (drop entries key)
              (let loop ((entries entries) (out '()))
                (cond ((null? entries) (reverse out))
                      ((eq? (caar entries) key) (loop (cdr entries) out))
                      (else (loop (cdr entries) (cons (car entries) out))))))
            (define* (get entries key (default '()))
              (let ((entry (and (list? entries) (assoc key entries))))
                (if entry (cadr entry) default)))
            (define (put entries key value)
              (cons (list key value) (drop entries key)))
            (define (strip-peer entry)
              (list (car entry)
                    (drop (drop (cadr entry) 'identity) 'identity-id)))
            (cond
             ((and (list? ledger-config) (list? public) (list? private)
                   (not (assoc 'key-derivation-salt public))
                   (byte-vector? identity-id) (= (length identity-id) 32)
                   (byte-vector? identity-nonce) (= (length identity-nonce) 32)
                   (equal? identity-id
                           (sync-hash
                            (expression->byte-vector
                             (list 'sync-web/journal-id/v1 identity-nonce))))
                   (equal? journal-key derived-journal)
                   (equal? interface-key derived-interface)
                   (equal? interface-key
                           (cadr (assoc 'public-key federation-config))))
              (set! public
                    (put (drop public 'identity)
                         'key-derivation-salt identity-id))
              (set! private (put private 'bridge-preapproval '()))
              (set! private (drop private 'bridge-identity))
              (if (assoc 'bridge private)
                  (set! private
                        (put private 'bridge
                             (map strip-peer (cadr (assoc 'bridge private))))))
              (if (assoc 'bridge-retired private)
                  (set! private
                        (put private 'bridge-retired
                             (map strip-peer
                                  (cadr (assoc 'bridge-retired private))))))
              (set! federation-config
                    (put federation-config 'peers
                         (map strip-peer (get federation-config 'peers))))
              (set! federation-config
                    (put federation-config 'retired
                         (map strip-peer (get federation-config 'retired))))
              ((old-ledger '~field!) 'config
               (expression->byte-vector
                `((public ,public) (private ,private))))
              ((old-federation '~field!) 'config
               (expression->byte-vector federation-config)))
             (released-1.6? #t)
             (else
              (error 'upgrade-error
                     "State is not an exact supported Interface version")))
            ((old-ledger '~field!) 'standard standard-node)
            ((old-federation '~field!) 'standard standard-node)
            ((root 'set!) '(root object standard) standard-node)
            ((root 'set!) '(root object ledger) (old-ledger))
            ((root 'set!) '(root object federation) (old-federation))
            #t))))

  (let loop ((rest classes)) (if (pair? rest) (begin (call `(lambda (root)
							      ((root 'set!) '(root object ,(caar rest)) ,(cadar rest))))
						     (loop (cdr rest)))))

  (call `(lambda (root) ((root 'set!) '(root handler secret) (expression->byte-vector
							      '(lambda (root old-secret new-secret)
                                                                   (if (equal? (sync-hash (expression->byte-vector new-secret))
                                                                               ((root 'get) '(interface secret)))
                                                                       (error 'argument-error "Root and Interface credentials must differ"))
								 (let* ((module (eval ((root 'get) '(root class standard-module))))
									(std-node ((root 'get) '(root object standard)))
									(standard (module 'local ((root 'get) '(root class standard)) std-node))
									(ledger (module 'local ((root 'get) '(root class ledger))
										((root 'get) '(root object ledger))))
									(salt ((ledger 'config) '(public key-derivation-salt)))
									(old (crypto-generate (expression->byte-vector
											       (list 'sync-web/journal-signing-key/v1 salt (sync-hash
																	  (expression->byte-vector old-secret))))))
									(new (crypto-generate (expression->byte-vector
											       (list 'sync-web/journal-signing-key/v1 salt (sync-hash
																	  (expression->byte-vector new-secret))))))
									(indexes ((ledger 'config) '(journal-rotation-indexes)))
									(previous (if (null? indexes) -1 (car (reverse indexes))))
									(index ((ledger 'size))))
								   (if (not (equal? (car old) (car new))) (begin ((ledger 'step!)
														  `((unix-time ,(system-time-unix))
														    (public-key ,(car new)) (secret-key ,(cdr new))
														    (rotation ((previous-key ,(car old)) (public-key ,(car new)) (signature
																						  ,(crypto-sign (cdr old) (expression->byte-vector
																									   (list 'sync-web/journal-key-rotation/v2 index
																										 previous (car old) (car new)))))))))
														 ((root 'set!) '(root object ledger) (ledger))))
                                                                   ((root 'set!) '(interface root-verifier)
                                                                    (sync-hash (expression->byte-vector new-secret)))
								   #t))))))

  (define query-once '(lambda (query) (let* ((function (cadr (assoc 'function query)))
					     (arguments (if (assoc 'arguments query) (cadr (assoc 'arguments query)) '()))
					     (authentication (and (assoc 'authentication query)
								  (cadr (assoc 'authentication query))))
					     (invocation (and (assoc 'invocation query) (cadr (assoc 'invocation query))))
                                             (delegation (and (assoc 'delegation query)
                                                              (cadr (assoc 'delegation query))))
					     (prepared (and (assoc 'prepared query) (cadr (assoc 'prepared query))))
					     (module (eval ((root 'get) '(root class standard-module))))
                                             (std-node ((root 'get) '(root object standard)))
                                             (standard (module 'local ((root 'get) '(root class standard)) std-node))
                                             (ledger (module 'local ((root 'get) '(root class ledger))
                                                             ((root 'get) '(root object ledger))))
                                             (federation (module 'local ((root 'get) '(root class federation))
                                                                 ((root 'get) '(root object federation))))
                                             (authorization (module 'local ((root 'get) '(root class authorization))
                                                                    ((root 'get) '(root object authorization))))
                                             (context '()) (auth-head #f) (ancestor? #f)
                                             (caller #f) (delegated? #f))

					(define (arg name) (let ((entry (assoc name arguments))) (and entry (cadr entry))))
                                        (define (transient-key domain secret)
                                          (let ((salt ((ledger 'config) '(public key-derivation-salt))))
                                            (crypto-generate
                                             (expression->byte-vector
                                              (list domain salt
                                                    (sync-hash
                                                     (expression->byte-vector secret)))))))
                                        (define (request-secret)
                                          (or (arg-from authentication 'credentials)
                                              (arg-from invocation 'credentials)
                                              (error 'authentication-error
                                                     "Signing requires local credentials")))
                                        (define (delegation-proof principal delegated-context
                                                                  delegated-function
                                                                  delegated-arguments)
                                          (sync-hash
                                           (expression->byte-vector
                                            (list 'sync-web/call-delegation/v1
                                                  ((root 'get) '(interface credential))
                                                  principal delegated-context
                                                  delegated-function delegated-arguments))))
					(define (set-alist values key value) (let loop ((in values) (out '()))
									       (cond ((null? in) (reverse (cons (list key value) out))) ((eq? (caar in) key)
																	 (append (reverse out) (cons (list key value) (cdr in))))
										     (else (loop (cdr in) (cons (car in) out))))))
                                        (define (local-admin? principal)
                                          (and (list? principal) (= (length principal) 2)
                                               (eq? (car principal) '*state*)
                                               (symbol? (cadr principal))))
                                        (define (admin-principals->map admins)
                                          (let loop ((principals admins) (keys '()) (entries '()))
                                            (cond ((null? principals) (reverse entries))
                                                  ((and (pair? principals)
                                                        (local-admin? (car principals))
                                                        (not (memq (cadar principals) keys)))
                                                   (loop (cdr principals)
                                                         (cons (cadar principals) keys)
                                                         (cons (list (cadar principals)
                                                                     (car principals))
                                                               entries)))
                                                  (else
                                                   (error 'integrity-error
                                                          "Stored admins must be unique local principals")))))
                                        (define (admin-map->principals admins)
                                          (let loop ((entries admins) (keys '()) (principals '()))
                                            (if (null? entries) (reverse principals)
                                                (if (and (pair? entries)
                                                         (list? (car entries))
                                                         (= (length (car entries)) 2)
                                                         (symbol? (caar entries))
                                                         (local-admin? (cadar entries))
                                                         (eq? (caar entries) (cadr (cadar entries)))
                                                         (not (memq (caar entries) keys)))
                                                    (loop (cdr entries)
                                                          (cons (caar entries) keys)
                                                          (cons (cadar entries) principals))
                                                    (error 'argument-error
                                                           "Admins must be a username-keyed map of local principals")))))
					(if (or (assoc 'meta arguments) (assoc 'metas arguments) (assoc 'meta? arguments))
					    (error 'argument-error "Metadata is not supported"))
                                        (if (and (memq function '(use! use-batch!))
                                                 (assoc 'read-only? arguments)
                                                 (not (boolean? (arg 'read-only?))))
                                            (error 'argument-error
                                                   "read-only? must be boolean"))
                                        (if (assoc 'index? arguments)
                                            (if (not (memq function '(retrieve retrieve-batch)))
                                                (error 'argument-error
                                                       "index? is only supported by retrieve operations")
                                                (if (not (boolean? (arg 'index?)))
                                                    (error 'argument-error
                                                           "index? must be boolean"))))
					(if (and (eq? function 'pin!) (assoc 'response arguments))
					    (error 'argument-error "Pin proof responses are internal"))
                                        (if (and (eq? function 'retrieve-batch)
                                                 (assoc 'proof? arguments)
                                                 (not invocation))
                                            (error 'argument-error
                                                   "Retrieve batch proofs are internal"))
					(define (with-auth request) (cond (authentication
									   (append request `((authentication ,authentication))))
									  (invocation (append request `((invocation ,invocation))))
									  (else request)))
					(define (self-call data)
					  (set! delegated? #t)
					  (sync-call (with-auth
					              `((function ,function) (arguments ,arguments)
					                ,@(if data `((prepared ,data)) '()))) #t))
                                        (define (all? predicate values)
                                          (let loop ((values values))
                                            (if (null? values) #t
                                                (if (predicate (car values))
                                                    (loop (cdr values)) #f))))
                                        (define (pin-batch-proofs data paths)
                                          (if (not (and (list? data) (= (length data) 2)
                                                        (list? (car data)) (= (length (car data)) 2)
                                                        (eq? (caar data) 'proofs)
                                                        (list? (cadr data)) (= (length (cadr data)) 2)
                                                        (eq? (caadr data) 'slots)))
                                              (error 'argument-error "Malformed prepared pin batch"))
                                          (let ((proofs (cadar data)) (slots (cadadr data)))
                                            (if (not (and (list? proofs) (list? slots)
                                                          (= (length slots) (length paths))
                                                          (all? (lambda (proof)
                                                                    (and (list? proof) (= (length proof) 2)
                                                                         (equal? (map car proof) '(proof index))
                                                                         (list? (cadr (car proof)))
                                                                         (integer? (cadr (cadr proof)))))
                                                                  proofs)
                                                          (all? (lambda (slot)
                                                                    (or (not slot)
                                                                        (and (integer? slot) (>= slot 0)
                                                                             (< slot (length proofs)))))
                                                                  slots)))
                                                (error 'argument-error "Malformed prepared pin batch"))
                                            (let loop ((index 0))
                                              (if (< index (length proofs))
                                                  (if (not (member index slots))
                                                      (error 'argument-error
                                                             "Unreferenced prepared pin proof")
                                                      (loop (+ index 1)))))
                                            data))
					(define (failure result) (let ((entry (and (pair? result) (pair? (car result))
										   (assoc 'failure result))))
								   (if entry (let ((value (cadr entry))) (error (cadr (assoc 'tag value))
														(cadr (assoc 'message value))))
								       result)))
					(define (federation-call method operands) (let* ((input (if (not prepared) operands
												    (append (reverse (cdr (reverse operands)))
													    (list `((value ,(car (reverse operands)))
														    (continuation ,prepared))))))
                                                                                         (input
                                                                                          (append input
                                                                                                  (list
                                                                                                   (transient-key
                                                                                                    'sync-web/federation-continuation-signing-key/v1
                                                                                                    (request-secret)))))
											 (result (apply (federation method) (cons ledger input)))
											 (next (and (pair? result) (pair? (car result)) (assoc 'continuation result))))
										    (cond (next (failure (self-call (cadr next)))) (prepared result)
											  (else (failure result)))))
                                        (define (authenticate)
                                          (if delegation
                                              (let ((principal (arg-from delegation 'principal))
                                                    (delegated-context
                                                     (arg-from delegation 'context))
                                                    (delegated-function
                                                     (arg-from delegation 'function))
                                                    (delegated-arguments
                                                     (arg-from delegation 'arguments))
                                                    (proof (arg-from delegation 'proof)))
                                                (if (or authentication invocation
                                                        (not
                                                         (and (list? delegation)
                                                              (= (length delegation) 5)
                                                              (list? principal)
                                                              (list? delegated-context)
                                                              (assoc 'latest-index delegated-context)
                                                              (integer?
                                                               (arg-from delegated-context
                                                                         'latest-index))
                                                              (or
                                                               (not
                                                                (assoc 'authentication-index
                                                                       delegated-context))
                                                               (integer?
                                                                (arg-from delegated-context
                                                                          'authentication-index)))
                                                              (eq? delegated-function function)
                                                              (equal? delegated-arguments arguments)
                                                              (equal?
                                                               proof
                                                               (delegation-proof
                                                                principal delegated-context
                                                                delegated-function
                                                                delegated-arguments)))))
                                                    (error 'authentication-error
                                                           "Invalid call delegation"))
                                                (set! context delegated-context)
                                                principal)
                                              (if invocation
                                                  (let ((source
                                                         (arg-from invocation 'route-source))
                                                        (target
                                                         (arg-from invocation 'route-target)))
                                                    (if (and (null? source) (null? target))
                                                        (let ((identity
                                                               (arg-from invocation 'identity)))
                                                          (if (not
                                                               (equal?
                                                                (sync-hash
                                                                 (expression->byte-vector
                                                                  (arg-from invocation
                                                                            'credentials)))
                                                                ((root 'get) '(interface secret))))
                                                              (error 'authentication-error
                                                                     "Authentication failed"))
                                                          (if (eq? identity '*journal*) '()
                                                              (if (eq? identity '*public*)
                                                                  '(*public*)
                                                                  `(*state* ,identity))))
                                                        (let* ((result
                                                                ((federation 'authenticate)
                                                                 ledger
                                                                 `((function ,function)
                                                                   (arguments ,arguments)
                                                                   (invocation ,invocation)))))
                                                          (set! context
                                                                (cadr (assoc 'context result)))
                                                          (if (assoc 'provider-paths result)
                                                              (let ((paths
                                                                     (cadr
                                                                      (assoc
                                                                       'provider-paths result))))
                                                                (set! arguments
                                                                      (set-alist
                                                                       arguments
                                                                       (if (eq? function
                                                                                'retrieve)
                                                                           'path 'paths)
                                                                       (if (eq? function
                                                                                'retrieve)
                                                                           (car paths) paths)))))
                                                          (set! auth-head
                                                                (and
                                                                 (assoc 'data-head result)
                                                                 `((index
                                                                    ,(cadr
                                                                      (assoc
                                                                       'data-head-index result)))
                                                                   (object
                                                                    ,(cadr
                                                                      (assoc
                                                                       'data-head result))))))
                                                          (cadr (assoc 'principal result)))))
                                                  (if (not authentication) '(*public*)
                                                      (let ((principal
                                                             (or
                                                              (arg-from authentication
                                                                        'identity)
                                                              '())))
                                                        (if (not
                                                             (equal?
                                                              (sync-hash
                                                               (expression->byte-vector
                                                                (arg-from authentication
                                                                          'credentials)))
                                                              ((root 'get)
                                                               '(interface secret))))
                                                            (error 'authentication-error
                                                                   "Authentication failed"))
                                                        principal)))))
					(define (arg-from values key) (let ((entry (and (list? values) (assoc key values))))
									(and entry (cadr entry))))
                                        (define retained-provider?
                                          (and invocation
                                               (pair? (arg-from invocation 'route-source))
                                               (null? (arg-from invocation 'route-target))
                                               (not (assoc 'data-object invocation))
                                               (memq function '(retrieve retrieve-batch))))
                                        (define (retained-path path)
                                          (define (reject)
                                            (error 'path-error
                                                   "Invalid retained path: ~S" path))
                                          (if (not (and (list? path) (pair? path)
                                                        (integer? (car path))
                                                        (pair? (cdr path))))
                                              (reject))
                                          (let loop ((segments (cdr path)) (route '())
                                                     (history (list (car path))))
                                            (cond
                                             ((null? segments)
                                              `((route ,route) (history ,history)
                                                (path ,path) (retained-provider? #t)))
                                             ((memq (car segments)
                                                    '(*state* *transition* *crypto*))
                                              `((route ,route) (history ,history)
                                                (path ,path) (retained-provider? #t)))
                                             ((and (eq? (car segments) '*bridge*)
                                                   (null? (cdr segments)))
                                              `((route ,route) (history ,history)
                                                (path ,path) (retained-provider? #t)))
                                             (else
                                              (let* ((explicit? (eq? (car segments) '*bridge*))
                                                     (name
                                                      (if explicit?
                                                          (and (pair? (cdr segments))
                                                               (cadr segments))
                                                          (car segments)))
                                                     (rest
                                                      (if explicit?
                                                          (and (pair? (cdr segments))
                                                               (cddr segments))
                                                          (cdr segments))))
                                                (if (not (and (symbol? name)
                                                              (not (memq name
                                                                         '(*state* *transition*
                                                                           *crypto* *bridge*)))
                                                              (list? rest)))
                                                    (reject))
                                                (let ((route (append route (list name))))
                                                  (if (null? rest)
                                                      `((route ,route) (history ,history)
                                                        (path ,path)
                                                        (retained-provider? #t))
                                                      (let* ((explicit-index?
                                                              (integer? (car rest)))
                                                             (index
                                                              (if explicit-index?
                                                                  (car rest) -1))
                                                             (rest
                                                              (if explicit-index?
                                                                  (cdr rest) rest)))
                                                        (loop rest route
                                                              (append history
                                                                      (list index)))))))))))
					(define (committed-path path)
					  (define (reject) (error 'path-error "Invalid committed path: ~S" path))
					  (if (not (and (list? path) (pair? path) (integer? (car path))
					                (pair? (cdr path)))) (reject))
					  (let ((origin-index (car path)) (tail (cdr path)))
					    (if (memq (car tail) '(*state* *transition* *crypto* *bridge*))
					        `((route ()) (history (,origin-index)) (path ,path))
					        (let loop ((segments tail) (route '()) (history (list origin-index)))
					          (if (not (and (pair? segments) (symbol? (car segments))
					                        (not (memq (car segments) '(*state* *transition* *crypto* *bridge*)))
					                        (pair? (cdr segments)) (integer? (cadr segments))
					                        (pair? (cddr segments))))
					              (reject))
					          (let ((alias (car segments)) (index (cadr segments))
					                (rest (cddr segments)))
					            (if (eq? (car rest) '*state*)
					                `((route ,(reverse (cons alias route)))
					                  (history ,(reverse (cons index history)))
					                  (path ,(cons index rest)))
					                (loop rest (cons alias route) (cons index history))))))))
                                        (define* (committed-indexes selected (proof #f))
                                          (let* ((route (arg-from selected 'route))
                                                 (history (arg-from selected 'history)))
                                            (if (and proof (pair? route))
                                                (let loop ((chain-node ((standard 'deserialize) proof))
                                                           (route route) (history history)
                                                           (indexes '()))
                                                  (let* ((selection
                                                          ((standard 'deep-call) chain-node '()
                                                           `(lambda (chain)
                                                              (let ((index
                                                                     ((chain 'index)
                                                                      ,(car history))))
                                                                (list index
                                                                      ((chain 'get) index))))))
                                                         (index (car selection))
                                                         (head (cadr selection))
                                                         (indexes (append indexes (list index))))
                                                    (if (null? route) indexes
                                                        (let ((nested
                                                               ((standard 'deep-get) head
                                                                `((*bridge* ,(car route) chain)))))
                                                          (if (or (not (sync-node? nested))
                                                                  (equal? nested '(nothing))
                                                                  (equal? nested '(unknown)))
                                                              (error 'integrity-error
                                                                     "Selected proof omits nested bridge chain")
                                                              (if (null? (cdr history)) indexes
                                                                  (loop nested (cdr route) (cdr history)
                                                                        indexes)))))))
                                                (if (pair? route)
                                                    (error 'integrity-error
                                                           "Federated index projection requires verified proof")
                                                    (let ((origin
                                                           (if (< (car history) 0)
                                                               (+ ((ledger 'size)) (car history))
                                                               (car history))))
                                                      (list origin))))))
                                        (define (indexed-result result selected)
                                          (let ((indexes
                                                 (committed-indexes
                                                  selected
                                                  (and (pair? result) (pair? (car result))
                                                       (or (assoc 'proof result)
                                                           (assoc 'indexed-proof result))
                                                       (cadr (or (assoc 'proof result)
                                                                 (assoc 'indexed-proof result)))))))
                                            (cond ((and (pair? result) (pair? (car result))
                                                        (assoc 'indexed-proof result))
                                                   `((content ,(cadr (assoc 'content result)))
                                                     (indexes ,indexes)))
                                                  ((and (pair? result) (pair? (car result))
                                                        (assoc 'content result))
                                                   (append
                                                    (let loop ((entries result) (public '()))
                                                      (cond ((null? entries) (reverse public))
                                                            ((memq (caar entries) '(proof-index indexes))
                                                             (loop (cdr entries) public))
                                                            (else
                                                             (loop (cdr entries)
                                                                   (cons (car entries) public)))))
                                                    `((indexes ,indexes))))
                                                  (else
                                                   `((content ,result) (indexes ,indexes))))))
					(define (origin-identity principal)
					  (cond ((equal? principal '(*public*)) '*public*)
					        ((null? principal) '*journal*)
					        ((and (pair? principal) (eq? (car principal) '*state*)
					              (pair? (cdr principal)) (symbol? (cadr principal))
					              (null? (cddr principal))) (cadr principal))
					        (else (error 'authentication-error "Invalid local federation identity"))))
                                        (define* (authorize-path principal path operation
                                                                 (operation-arguments '()))
                                          (let* ((admins ((root 'get) '(interface admins)))
                                                 (remote (assoc 'authentication-index context))
                                                 (direct?
                                                  (and (not remote)
                                                       (or (null? principal)
                                                           (member principal admins))))
                                                 (ctx `((latest-index ,(- ((ledger 'size)) 1))
                                                        ,@(if remote
                                                              `((authentication-index
                                                                 ,(cadr remote))) '()))))
                                            (if direct? 'direct
                                                (and path
                                                     ((authorization 'authorized?)
                                                      principal ctx path operation
                                                      operation-arguments)))))
                                        (define* (authorize-batch paths operation
                                                                  (operation-arguments '()))
                                          (if (not (list? paths))
                                              (error 'argument-error
                                                     "Batch paths must be a proper list"))
                                          (if (> (length paths) 1024)
                                              (error 'argument-error
                                                     "Batch path count exceeds 1024"))
                                          (let ((principal (authenticate)))
                                            (let loop ((paths paths) (ancestors '()))
                                              (if (null? paths)
                                                  (list principal (reverse ancestors))
                                                  (let ((decision
                                                         (authorize-path principal
                                                                         (car paths)
                                                                         operation operation-arguments)))
                                                    (if (not decision)
                                                        (error 'authorization-error
                                                               "Not authorized"))
                                                    (loop (cdr paths)
                                                          (cons (eq? decision 'ancestor)
                                                                ancestors)))))))
                                        (define* (authorize (authenticated-principal #f))
                                          (let* ((principal (or authenticated-principal
                                                                (authenticate)))
                                                 (path
                                                  (let ((path (arg 'path)))
                                                    (if (and (eq? function 'trace)
                                                             (arg 'index)
                                                             (or (null? path)
                                                                 (not (integer? (car path)))))
                                                        (cons (arg 'index) path) path)))
                                                 (admins ((root 'get) '(interface admins)))
                                                 (remote (assoc 'authentication-index context))
                                                 (owner (arg 'user))
                                                 (conditional? (assoc 'expected arguments))
                                                 (periodic-write?
                                                  (and (memq function '(put! put-batch! copy! copy-batch!))
                                                       (if (memq function '(put! copy!))
                                                           (equal? path '(*state* *periodic*))
                                                           (let loop ((paths (arg 'paths)))
                                                             (and (pair? paths)
                                                                  (or (equal? (car paths)
                                                                              '(*state* *periodic*))
                                                                      (loop (cdr paths))))))))
                                                 (direct?
                                                  (or (and (not remote)
                                                           (or (null? principal)
                                                               (member principal admins)))
                                                      (and (memq function
                                                                 '(authorizations authorize!
                                                                   deauthorize!))
                                                           owner
                                                           (equal? owner principal))))
                                                 (decision
                                                  (cond
                                                   (periodic-write? (and direct? 'direct))
                                                   ((memq function '(truncate! prune! prune-batch!))
                                                    (and direct? 'direct))
                                                   ((eq? function 'put-batch!)
                                                    (let ((writes
                                                           (authorize-batch
                                                            (arg 'paths) 'put!))
                                                          (reads
                                                           (and conditional?
                                                                (authorize-batch
                                                                 (arg 'paths) 'use!
                                                                 '((read-only? #t))))))
                                                      (and (not (member #t (cadr writes)))
                                                           (or (not reads)
                                                               (not (member #t
                                                                            (cadr reads))))
                                                           'direct)))
                                                   ((eq? function 'use-batch!)
                                                    (let ((uses
                                                           (authorize-batch
                                                            (arg 'paths) 'use!
                                                            `((read-only?
                                                               ,(not (not (arg 'read-only?))))))))
                                                      (and (not (member #t (cadr uses)))
                                                           'direct)))
                                                   ((eq? function 'copy-batch!)
                                                    (let ((source-reads
                                                           (authorize-batch
                                                            (arg 'sources) 'use!
                                                            '((read-only? #t))))
                                                          (target-writes
                                                           (authorize-batch
                                                            (arg 'paths) 'put!))
                                                          (target-reads
                                                           (and conditional?
                                                                (authorize-batch
                                                                 (arg 'paths) 'use!
                                                                 '((read-only? #t))))))
                                                      (and (not (member #t (cadr source-reads)))
                                                           (not (member #t (cadr target-writes)))
                                                           (or (not target-reads)
                                                               (not (member #t
                                                                            (cadr target-reads))))
                                                           'direct)))
                                                   ((and (eq? function 'put!) conditional?)
                                                    (let ((write
                                                           (authorize-path principal path 'put!))
                                                          (read
                                                           (authorize-path principal path 'use!
                                                                           '((read-only? #t)))))
                                                      (and write read
                                                           (not (eq? write 'ancestor))
                                                           (not (eq? read 'ancestor))
                                                           'direct)))
                                                   ((eq? function 'copy!)
                                                    (let ((source-read
                                                           (authorize-path principal
                                                                           (arg 'source) 'use!
                                                                           '((read-only? #t))))
                                                          (target-write
                                                           (authorize-path principal path 'put!))
                                                          (target-read
                                                           (and conditional?
                                                                (authorize-path principal path 'use!
                                                                                '((read-only? #t))))))
                                                      (and source-read target-write
                                                           (or (not conditional?) target-read)
                                                           (not (eq? source-read 'ancestor))
                                                           (not (eq? target-write 'ancestor))
                                                           (or (not conditional?)
                                                               (not (eq? target-read 'ancestor)))
                                                           'direct)))
                                                   (direct? 'direct)
                                                   (else
                                                    (authorize-path
                                                     principal path function
                                                     (if (eq? function 'use!)
                                                         `((read-only?
                                                            ,(not (not (arg 'read-only?)))))
                                                         '()))))))
                                            (if (not decision)
                                                (error 'authorization-error "Not authorized"))
                                            (set! ancestor? (eq? decision 'ancestor))
                                            (set! caller principal)
                                            principal))
					(define (encode value expression?) (cond ((equal? value '(nothing)) value)
										 (expression? (expression->byte-vector value))
										 ((byte-vector? value) value)
										 (else (error 'value-error "Expected byte-vector"))))
					(define (decode value expression?) (cond ((not expression?) value)
										 ((byte-vector? value) (byte-vector->expression value))
										 ((and (pair? value) (pair? (car value)) (assoc 'content value))
										  (set-alist value 'content (decode (cadr (assoc 'content value)) #t)))
										 (else value)))
                                        (define (resource-result value expression?)
                                          (define (inert? value)
                                            (cond ((or (sync-node? value) (procedure? value)
                                                       (macro? value)) #f)
                                                  ((byte-vector? value) #t)
                                                  ((pair? value)
                                                   (and (not (eq? (car value) 'sync-node))
                                                        (inert? (car value))
                                                        (inert? (cdr value))))
                                                  ((vector? value) #f)
                                                  (else #t)))
                                          (cond ((equal? value '(unknown)) value)
                                                (expression?
                                                 (if (not (inert? value))
                                                     (error 'value-error
                                                            "Resource result cannot expose runtime objects"))
                                                 (expression->byte-vector value)
                                                 value)
                                                ((byte-vector? value) value)
                                                (else
                                                 (error 'value-error
                                                        "Resource result must match the expression codec"))))
					(define* (stage-directory value (admitted? ancestor?)) (if (not admitted?) value
									    (let ((entries (and (pair? value) (eq? (car value) 'directory) (cadr value))))
									      (if (not entries) (error 'authorization-error "Not a directory"))
									      `(directory ,(let loop ((in entries) (out '())) (if (null? in) (reverse out)
																  (let* ((name (caar in)) (text (symbol->string name))) (loop (cdr in)
																							      (if (and (> (length text) 1) (char=? (text 0) #\*)
																								       (char=? (text (- (length text) 1)) #\*))
																								  out (cons (car in) out)))))) #f))))
					(define* (hydrate path (index -1))
					  ((federation 'route) ledger
					   `((operation hydrate) (path ,path) (index ,index))))
					(define (federate) (if (not (memq function '(put! copy! use! put-batch! copy-batch! use-batch!
                                                                                       run! retrieve retrieve-batch)))
							       (error 'api-error "Not federated"))
					  (if (not (equal? (sync-hash
							    (expression->byte-vector (arg-from invocation 'credentials)))
							   ((root 'get) '(interface secret))))
					      (error 'authentication-error "Authentication failed"))
                                          (if (eq? function 'retrieve-batch)
                                              `((results
                                                 ,((federation 'invoke-batch) ledger arguments
                                                   (arg-from invocation 'route-target)
                                                   (arg-from invocation 'history-indexes)
                                                   (arg-from invocation 'identity)
                                                   (transient-key
                                                    'sync-web/interface-signing-key/v1
                                                    (request-secret)))))
                                              ((federation 'invoke) ledger function arguments
                                               (arg-from invocation 'route-target)
                                               (arg-from invocation 'history-indexes)
                                               (arg-from invocation 'identity)
                                               (transient-key
                                                'sync-web/interface-signing-key/v1
                                                (request-secret)))))
					(define* (committed-invoke operation principal selected invoke-arguments
					                           (indexed-proof? #f))
					  ((federation 'invoke) ledger operation
					   (set-alist invoke-arguments 'path (arg-from selected 'path))
					   (arg-from selected 'route) (arg-from selected 'history)
					   (origin-identity principal)
                                           (transient-key 'sync-web/interface-signing-key/v1
                                                          (request-secret))
					   indexed-proof?))
                                        (define (committed-groups paths)
                                          (if (not (list? paths))
                                              (error 'argument-error
                                                     "Batch paths must be a proper list"))
                                          (if (> (length paths) 1024)
                                              (error 'argument-error
                                                     "Batch path count exceeds 1024"))
                                          (let loop ((paths paths) (index 0) (groups '()))
                                            (if (null? paths) groups
                                                (let* ((selected
                                                        (committed-path (car paths)))
                                                       (route
                                                        (arg-from selected 'route))
                                                       (history
                                                        (arg-from selected 'history))
                                                       (item
                                                        `((index ,index)
                                                          (path ,(car paths))
                                                          (selected ,selected))))
                                                  (let insert ((groups groups) (out '()))
                                                    (cond
                                                     ((null? groups)
                                                      (loop
                                                       (cdr paths) (+ index 1)
                                                       (append
                                                        (reverse out)
                                                        `(((route ,route)
                                                           (history ,history)
                                                           (items (,item)))))))
                                                     ((and
                                                       (equal?
                                                        (arg-from (car groups) 'route)
                                                        route)
                                                       (equal?
                                                        (arg-from (car groups) 'history)
                                                        history))
                                                      (let ((updated
                                                             (set-alist
                                                              (car groups) 'items
                                                              (append
                                                               (arg-from
                                                                (car groups) 'items)
                                                               (list item)))))
                                                        (loop
                                                         (cdr paths) (+ index 1)
                                                         (append
                                                          (reverse out)
                                                          (cons updated
                                                                (cdr groups))))))
                                                     (else
                                                      (insert (cdr groups)
                                                              (cons (car groups)
                                                                    out)))))))))
                                        (define (call-program program)
                                          (define (split-keywords values)
                                            (let loop ((remaining values) (positionals '())
                                                       (keywords '()))
                                              (cond
                                               ((null? remaining)
                                                (list (reverse positionals) (reverse keywords)))
                                               ((keyword? (car remaining))
                                                (if (null? (cdr remaining))
                                                    (error 'argument-error
                                                           "Missing value for keyword: ~S"
                                                           (car remaining)))
                                                (loop (cddr remaining) positionals
                                                      (cons (list
                                                             (keyword->symbol (car remaining))
                                                             (cadr remaining))
                                                            keywords)))
                                               (else
                                                (loop (cdr remaining)
                                                      (cons (car remaining) positionals)
                                                      keywords)))))
                                          (define (api-arguments method values)
                                            (let* ((split (split-keywords values))
                                                   (positionals (car split))
                                                   (keywords (cadr split)))
                                              (case method
                                                ((put!)
                                                 (let ((result
                                                        (cond
                                                         ((>= (length positionals) 2)
                                                          (append
                                                           `((path ,(car positionals))
                                                             (value ,(cadr positionals)))
                                                           keywords))
                                                         ((= (length positionals) 1)
                                                          (cons `(path ,(car positionals))
                                                                keywords))
                                                         ((assoc 'path keywords) keywords)
                                                         (else
                                                          (error 'argument-error
                                                                 "put! requires a path")))))
                                                   (if (or (assoc 'expression? result)
                                                           (not (assoc 'value result)))
                                                       result
                                                       (cons '(expression? #t) result))))
                                                ((copy!)
                                                 (let ((result
                                                        (cond
                                                         ((>= (length positionals) 2)
                                                          (append
                                                           `((source ,(car positionals))
                                                             (path ,(cadr positionals)))
                                                           keywords))
                                                         ((and (assoc 'source keywords)
                                                               (assoc 'path keywords))
                                                          keywords)
                                                         (else
                                                          (error 'argument-error
                                                                 "copy! requires source and path")))))
                                                   (if (or (assoc 'expression? result)
                                                           (not (assoc 'expected result)))
                                                       result
                                                       (cons '(expression? #t) result))))
                                                ((use! retrieve)
                                                 (let ((result
                                                        (cond
                                                         ((and (= (length positionals) 1)
                                                               (list? (car positionals))
                                                               (pair? (car positionals))
                                                               (pair? (caar positionals))
                                                               (assoc 'path (car positionals)))
                                                          (append (car positionals) keywords))
                                                         ((pair? positionals)
                                                          (cons `(path ,(car positionals))
                                                                keywords))
                                                         ((assoc 'path keywords) keywords)
                                                         (else
                                                          (error 'argument-error
                                                                 "~S requires a path" method)))))
                                                   (let ((result
                                                          (if (assoc 'expression? result) result
                                                              (cons '(expression? #t) result))))
                                                     (if (and (eq? method 'use!)
                                                              (pair? positionals)
                                                              (not (assoc 'read-only? result))
                                                              (not (assoc 'method result)))
                                                         (cons '(read-only? #t) result)
                                                         result))))
                                                ((use-batch!)
                                                 (if (and (= (length positionals) 1)
                                                          (list? (car positionals)))
                                                     (let unwrap ((payload (car positionals)))
                                                       (if (and (= (length payload) 1)
                                                                (list? (car payload))
                                                                (or (null? (car payload))
                                                                    (not (and (pair? (caar payload))
                                                                              (symbol? (caaar payload))))))
                                                           (unwrap (car payload))
                                                           (append payload keywords)))
                                                     (error 'argument-error
                                                            "Use named arguments for use-batch!")))
                                                ((retrieve-batch pin-batch! unpin-batch! prune-batch!)
                                                 (let ((result
                                                        (cond
                                                         ((pair? positionals)
                                                          (cons `(paths ,(car positionals))
                                                                keywords))
                                                         ((assoc 'paths keywords) keywords)
                                                         (else
                                                          (error 'argument-error
                                                                 "~S requires paths" method)))))
                                                   (if (and (eq? method 'retrieve-batch)
                                                            (not (assoc 'expression? result)))
                                                       (cons '(expression? #t) result)
                                                       result)))
                                                ((run!)
                                                 (cond
                                                  ((>= (length positionals) 2)
                                                   (append
                                                    `((path ,(car positionals))
                                                      (arguments ,(cadr positionals)))
                                                    keywords))
                                                  ((and (assoc 'path keywords)
                                                        (assoc 'arguments keywords))
                                                   keywords)
                                                  (else
                                                   (error 'argument-error
                                                          "run! requires a path and arguments list"))))
                                                ((pin! unpin! prune! delete-bridge!)
                                                 (if (null? positionals)
                                                     (error 'argument-error
                                                            "~S requires one argument" method))
                                                 (cons
                                                  (list
                                                   (if (eq? method 'delete-bridge!) 'name 'path)
                                                   (car positionals))
                                                  keywords))
                                                (else
                                                 (cond
                                                  ((and (eq? method 'put!)
                                                        (>= (length positionals) 2))
                                                   (append
                                                    `((path ,(car positionals))
                                                      (value ,(cadr positionals))
                                                      (expression? #t))
                                                    keywords))
                                                  ((and (eq? method 'use!)
                                                        (pair? positionals))
                                                   (append
                                                    `((path ,(car positionals))
                                                      (expression? #t))
                                                    keywords))
                                                  ((null? positionals) keywords)
                                                  ((and (= (length positionals) 1)
                                                        (list? (car positionals)))
                                                   (append (car positionals) keywords))
                                                  (else
                                                   (error 'argument-error
                                                          "Use named arguments for ~S: ~S"
                                                          method values)))))))
                                          (define (safe-program-datum? value)
                                            (cond
                                             ((or (null? value) (boolean? value)
                                                  (number? value) (char? value)
                                                  (string? value) (symbol? value)
                                                  (keyword? value) (byte-vector? value)
                                                  (sync-node? value)) #t)
                                             ((syntax? value)
                                              (let loop
                                                  ((names
                                                    '(and begin case cond define define* do else if
                                                      lambda lambda* let let* letrec letrec* or
                                                      quasiquote quote set! unquote
                                                      unquote-splicing)))
                                                (and (pair? names)
                                                     (or (eq? value ((rootlet) (car names)))
                                                         (loop (cdr names))))))
                                             ((pair? value)
                                              (and (safe-program-datum? (car value))
                                                   (safe-program-datum? (cdr value))))
                                             ((vector? value)
                                              (let loop ((index 0))
                                                (or (= index (vector-length value))
                                                    (and (safe-program-datum?
                                                          (vector-ref value index))
                                                         (loop (+ index 1))))))
                                             (else #f)))
                                          (define (forbidden-program-symbol? value)
                                            (cond
                                             ((symbol? value)
                                              (memq value
                                                    '(*rootlet-redefinition-hook*
                                                      *read-error-hook* *error-hook*
                                                      *autoload-hook* *load-hook*
                                                      *missing-close-paren-hook*
                                                      *unbound-variable-hook* *s7*
                                                      let-set! let-ref unlet)))
                                             ((pair? value)
                                              (or (forbidden-program-symbol? (car value))
                                                  (forbidden-program-symbol? (cdr value))))
                                             ((vector? value)
                                              (let loop ((index 0))
                                                (and (< index (vector-length value))
                                                     (or (forbidden-program-symbol?
                                                          (vector-ref value index))
                                                         (loop (+ index 1))))))
                                             (else #f)))
                                          (define (safe-result? value)
                                            (cond ((or (null? value) (boolean? value)
                                                       (number? value) (char? value)
                                                       (string? value) (symbol? value)
                                                       (keyword? value) (byte-vector? value)
                                                       (sync-node? value)) #t)
                                                  ((pair? value)
                                                   (and (safe-result? (car value))
                                                        (safe-result? (cdr value))))
                                                  ((vector? value)
                                                   (let loop ((index 0))
                                                     (or (= index (vector-length value))
                                                         (and (safe-result?
                                                               (vector-ref value index))
                                                              (loop (+ index 1))))))
                                                  (else #f)))
                                          (let ((call-arguments (arg 'arguments)))
                                            (if (not (safe-program-datum? program))
                                                (error 'value-error
                                                       "run! program contains a captured evaluator object"))
                                            (if (forbidden-program-symbol? program)
                                                (error 'value-error
                                                       "run! program names a forbidden evaluator binding"))
                                            (if (not (proper-list? call-arguments))
                                                (error 'argument-error
                                                       "run! arguments must be a proper list"))
                                            (if (not (safe-program-datum? call-arguments))
                                                (error 'value-error
                                                       "run! arguments contain an evaluator capability"))
                                            (let* ((environment (inlet))
                                                   (root-environment (rootlet))
                                                   (capabilities
                                                    '(* + - / < <= = > >= and append apply
                                                      apply-values ash assq assoc begin boolean?
                                                      byte-vector? byte-vector->hex-string
                                                      byte-vector-length
                                                      byte-vector-ref byte-vector-set! caadar caadr
                                                      caar cadar cadr caddr car case catch cdadar
                                                      cddar cddr cdr char? complex? cond cons define
                                                      define* do else eq? equal? eqv? error even? expt
                                                      for-each if integer?
                                                      keyword? keyword->symbol lambda lambda* length
                                                      let let* letrec letrec* list list-tail list-values
                                                      list? logand macro? make-list map max member memq
                                                      min modulo negative? not null? number->string
                                                      number? odd? or pair? positive? procedure?
                                                      proper-list? quasiquote quote rational? real?
                                                      remainder reverse set! string->symbol string?
                                                      string=? string-length string-ref substring
                                                      subvector symbol->string symbol? throw unquote
                                                      unquote-splicing vector vector-length vector-ref
                                                      zero?)))
                                              (for-each
                                               (lambda (binding)
                                                 (catch #t
                                                   (lambda ()
                                                     (varlet environment
                                                             (car binding) '*removed*))
                                                   (lambda args #f)))
                                               root-environment)
                                              (for-each
                                               (lambda (name)
                                                 (varlet environment name
                                                         (root-environment name)))
                                               capabilities)
                                              (coverlet environment)
                                              (let* ((procedure (eval program environment))
                                                     (journal
                                                      (lambda (method)
                                                        (if (not (symbol? method))
                                                            (error 'argument-error
                                                                   "Journal method must be a symbol: ~S"
                                                                   method))
                                                        (lambda values
                                                          (let ((call-arguments
                                                                 (api-arguments method values)))
                                                            (if (not (safe-program-datum? call-arguments))
                                                                (error 'value-error
                                                                       "Journal arguments cannot expose capabilities"))
                                                            (set! delegated? #t)
                                                            (let* ((request
                                                                    `((function ,method)
                                                                      (arguments ,call-arguments)))
                                                                   (request
                                                                    (if (or invocation delegation)
                                                                        (append
                                                                         request
                                                                         `((delegation
                                                                            ((principal ,caller)
                                                                             (context ,context)
                                                                             (function ,method)
                                                                             (arguments ,call-arguments)
                                                                             (proof
                                                                              ,(delegation-proof
                                                                                caller context method
                                                                                call-arguments))))))
                                                                        (with-auth request)))
                                                                   (result (sync-call request #t)))
                                                              (if (and (pair? result)
                                                                       (eq? (car result) 'error))
                                                                  (apply error (cdr result))
                                                                  result)))))))
                                                (if (not (procedure? procedure))
                                                    (error 'value-error
                                                           "run! path must contain a procedure"))
                                                (set! delegated? #t)
                                                (let ((result
                                                       (apply procedure
                                                              (cons journal call-arguments))))
                                                  (if (not (safe-result? result))
                                                      (error 'value-error
                                                             "run! result cannot expose capabilities")
                                                      result))))))
					(define (config-view path)
  (if (not (and (pair? path) (eq? (car path) 'private)))
      ((ledger 'config) path)
      (case (and (pair? (cdr path)) (cadr path))
        ((bridge bridge-retired)
         ((federation 'config)
          (cons (if (eq? (cadr path) 'bridge) 'peers 'retired)
                (cddr path))))
        ((bridge-preapproval)
         ((ledger 'config) (cons 'peer-preapproval (cddr path))))
        (else '()))))

					(if (and invocation (pair? (arg-from invocation 'route-target))) (federate)
					    (let ((result (case function ((route) ((federation 'route) ledger
										   `((route-source ,(or (arg 'route-source) '()))
										     (route-target ,(arg 'route-target))
										     (index ,(or (arg 'index) -1))
										     ,@(if (assoc 'history-indexes arguments)
											   `((history-indexes ,(arg 'history-indexes))) '())
										     ,@(if (assoc 'history-head-index arguments) `((history-head-index
																    ,(arg 'history-head-index))) '()))))
								((size) ((ledger 'size)))
								((info) ((ledger 'descriptor) -1))
								((synchronize!) ((federation 'synchronize!) ledger arguments))
								(else
                                                                 (if (memq function
                                                                           '(retrieve-batch pin-batch! unpin-batch!
                                                                             trace-batch))
                                                                     (case function
                                                                       ((retrieve-batch)
                                                                        (let* ((requested-paths (arg 'paths))
                                                                               (preauthenticated
                                                                                (and retained-provider?
                                                                                     (authenticate)))
                                                                               (paths (arg 'paths))
                                                                               (groups
                                                                                (if retained-provider?
                                                                                    (let loop ((paths paths)
                                                                                               (index 0)
                                                                                               (items '()))
                                                                                      (if (null? paths)
                                                                                          `(((route ()) (history ())
                                                                                             (items ,(reverse items))))
                                                                                          (loop
                                                                                           (cdr paths) (+ index 1)
                                                                                           (cons
                                                                                            `((index ,index)
                                                                                              (path ,(car paths))
                                                                                              (selected
                                                                                               ,(retained-path
                                                                                                 (car paths))))
                                                                                            items))))
                                                                                    (committed-groups paths)))
                                                                               (principal
                                                                                (or preauthenticated
                                                                                    (authenticate)))
                                                                               (slots
                                                                                (make-vector
                                                                                 (length paths) #f))
                                                                               (internal-proof?
                                                                                (and invocation
                                                                                     (arg 'proof?)))
                                                                               (proof #f))
                                                                          ;; Reject every locally decidable denial before
                                                                          ;; any remote group can perform external work.
                                                                          (for-each
                                                                           (lambda (group)
                                                                             (if (null?
                                                                                  (arg-from group
                                                                                            'route))
                                                                                 (for-each
                                                                                  (lambda (item)
                                                                                    (if (not
                                                                                         (authorize-path
                                                                                          principal
                                                                                          (arg-from item
                                                                                                    'path)
                                                                                          'retrieve))
                                                                                        (error
                                                                                         'authorization-error
                                                                                         "Not authorized")))
                                                                                  (arg-from group
                                                                                            'items))))
                                                                           groups)
                                                                          (for-each
                                                                           (lambda (group)
                                                                             (let* ((route
                                                                                     (arg-from group
                                                                                               'route))
                                                                                    (history
                                                                                     (arg-from group
                                                                                               'history))
                                                                                    (items
                                                                                     (arg-from group
                                                                                               'items))
                                                                                    (group-paths
                                                                                     (map
                                                                                      (lambda (item)
                                                                                        (arg-from item
                                                                                                  'path))
                                                                                      items))
                                                                                    (group-methods
                                                                                     (and (arg 'methods)
                                                                                          (map (lambda (item)
                                                                                                 (car (list-tail
                                                                                                       (arg 'methods)
                                                                                                       (arg-from item 'index))))
                                                                                               items)))
                                                                                    (group-arguments
                                                                                     (and (arg 'arguments)
                                                                                          (map (lambda (item)
                                                                                                 (car (list-tail
                                                                                                       (arg 'arguments)
                                                                                                       (arg-from item 'index))))
                                                                                               items)))
                                                                                    (group-request
                                                                                     (let ((request
                                                                                            (set-alist arguments 'paths
                                                                                                       (map
                                                                                                        (lambda (item)
                                                                                                          (arg-from
                                                                                                           (arg-from item 'selected)
                                                                                                           'path))
                                                                                                        items))))
                                                                                       (if group-methods
                                                                                           (set! request
                                                                                                 (set-alist request 'methods
                                                                                                            group-methods)))
                                                                                       (if group-arguments
                                                                                           (set! request
                                                                                                 (set-alist request 'arguments
                                                                                                            group-arguments)))
                                                                                       request))
                                                                                    (ancestors
                                                                                     (if (pair? route)
                                                                                         (map
                                                                                          (lambda (path)
                                                                                            #f)
                                                                                          group-paths)
                                                                                         (map
                                                                                          (lambda (path)
                                                                                            (let ((decision
                                                                                                   (authorize-path
                                                                                                    principal
                                                                                                    path
                                                                                                    'retrieve)))
                                                                                              (if (not decision)
                                                                                                  (error
                                                                                                   'authorization-error
                                                                                                   "Not authorized"))
                                                                                              (eq? decision
                                                                                                   'ancestor)))
                                                                                          group-paths)))
                                                                                    (values
                                                                                     (if (pair? route)
                                                                                         ((federation
                                                                                           'invoke-batch)
                                                                                          ledger
                                                                                          group-request
                                                                                          route history
                                                                                          (origin-identity
                                                                                           principal)
                                                                                          (transient-key
                                                                                           'sync-web/interface-signing-key/v1
                                                                                           (request-secret))
                                                                                          (or internal-proof?
                                                                                              (arg 'index?)))
                                                                                         ((ledger
                                                                                           'retrieve-batch)
                                                                                          group-paths #f
                                                                                          (and auth-head
                                                                                               (map
                                                                                                (lambda (path)
                                                                                                  auth-head)
                                                                                                group-paths))
                                                                                          ancestors
                                                                                          internal-proof?
                                                                                          group-methods group-arguments)))
                                                                                    (returned-proof
                                                                                     (and (list? values)
                                                                                          (assoc 'proof values)
                                                                                          (cadr (assoc 'proof values))))
                                                                                    (indexes
                                                                                     (and
                                                                                      (arg 'index?)
                                                                                      (committed-indexes
                                                                                       (arg-from
                                                                                        (car items)
                                                                                        'selected)
                                                                                       returned-proof)))
                                                                                    (group-proof
                                                                                     (and internal-proof?
                                                                                          returned-proof))
                                                                                    (values
                                                                                     (if returned-proof
                                                                                         (cadr
                                                                                          (assoc 'results
                                                                                                 values))
                                                                                         values)))
                                                                               (if group-proof
                                                                                   (if proof
                                                                                       (error 'integrity-error
                                                                                              "Terminal batch produced multiple proofs")
                                                                                       (set! proof group-proof)))
                                                                               (let loop ((items items)
                                                                                          (values values))
                                                                                 (if (pair? items)
                                                                                     (begin
                                                                                       (vector-set!
                                                                                        slots
                                                                                        (arg-from
                                                                                         (car items)
                                                                                         'index)
                                                                                        (if indexes
                                                                                            (set-alist
                                                                                             (car values)
                                                                                             'indexes
                                                                                             (if retained-provider?
                                                                                                 (committed-indexes
                                                                                                  (arg-from
                                                                                                   (car items)
                                                                                                   'selected)
                                                                                                  returned-proof)
                                                                                                 indexes))
                                                                                            (car values)))
                                                                                       (loop
                                                                                        (cdr items)
                                                                                        (cdr values)))))))
                                                                           groups)
                                                                          (let loop ((paths paths)
                                                                                     (requested-paths requested-paths)
                                                                                     (index 0)
                                                                                     (results '()))
                                                                            (if (null? paths)
                                                                                `((results
                                                                                   ,(reverse results))
                                                                                  ,@(if proof
                                                                                        `((proof ,proof))
                                                                                        '()))
                                                                                (let* ((slot
                                                                                        (vector-ref
                                                                                         slots index))
                                                                                       (content
                                                                                        (let ((value
                                                                                               (decode
                                                                                                (cadr
                                                                                                 (assoc
                                                                                                  'content
                                                                                                  slot))
                                                                                                (arg
                                                                                                 'expression?))))
                                                                                          (if (or (arg 'methods)
                                                                                                  (arg 'arguments))
                                                                                              (resource-result
                                                                                               value
                                                                                               (arg 'expression?))
                                                                                              value))))
                                                                                  (loop
                                                                                   (cdr paths)
                                                                                   (cdr requested-paths)
                                                                                   (+ index 1)
                                                                                   (cons
                                                                                    `((path ,(car requested-paths))
                                                                                      (content ,content)
                                                                                      ,@(if (arg 'pinned?)
                                                                                            `((pinned?
                                                                                               ,((ledger
                                                                                                  'pinned?)
                                                                                                 (car paths))))
                                                                                            '())
                                                                                      ,@(if (arg 'index?)
                                                                                            `((indexes
                                                                                               ,(arg-from
                                                                                                 slot
                                                                                                 'indexes)))
                                                                                            '()))
                                                                                    results)))))))
                                                                       ((pin-batch!)
                                                                        (let ((paths (arg 'paths)))
                                                                          (authorize-batch paths 'pin!)
                                                                          (if prepared
                                                                              ((ledger 'pin-batch!)
                                                                               paths
                                                                               (pin-batch-proofs prepared paths))
                                                                              (let* ((groups
                                                                                      (committed-groups
                                                                                       paths))
                                                                                     (principal
                                                                                      (authenticate))
                                                                                     (proof-slots
                                                                                      (make-vector
                                                                                       (length paths)
                                                                                       #f))
                                                                                     (proof-groups '())
                                                                                     (remote? #f))
                                                                                ;; Retrieve every remote proof group in
                                                                                ;; this read-only phase. A blocking
                                                                                ;; self-call performs the later atomic
                                                                                ;; retention mutation.
                                                                                (for-each
                                                                                 (lambda (group)
                                                                                   (let* ((route
                                                                                           (arg-from
                                                                                            group 'route))
                                                                                          (history
                                                                                           (arg-from
                                                                                            group 'history))
                                                                                          (items
                                                                                           (arg-from
                                                                                            group 'items)))
                                                                                     (if (pair? route)
                                                                                         (let* ((_ (set! remote?
                                                                                                         #t))
                                                                                                (retrieved
                                                                                                 ((federation
                                                                                                   'invoke-batch)
                                                                                                  ledger
                                                                                                  (set-alist
                                                                                                   arguments
                                                                                                   'paths
                                                                                                   (map
                                                                                                    (lambda
                                                                                                     (item)
                                                                                                      (arg-from
                                                                                                       (arg-from
                                                                                                        item
                                                                                                        'selected)
                                                                                                       'path))
                                                                                                    items))
                                                                                                  route history
                                                                                                  (origin-identity
                                                                                                   principal)
                                                                                                  (transient-key
                                                                                                   'sync-web/interface-signing-key/v1
                                                                                                   (request-secret))
                                                                                                  #t))
                                                                                                (proof
                                                                                                 `((proof
                                                                                                    ,(cadr
                                                                                                      (assoc
                                                                                                       'proof
                                                                                                       retrieved)))
                                                                                                   (index
                                                                                                    ,(cadr
                                                                                                      (assoc
                                                                                                       'proof-index
                                                                                                       retrieved)))))
                                                                                                (slot
                                                                                                 (length
                                                                                                  proof-groups)))
                                                                                           (set! proof-groups
                                                                                                 (append proof-groups
                                                                                                         (list proof)))
                                                                                           (for-each
                                                                                            (lambda (item)
                                                                                              (vector-set!
                                                                                               proof-slots
                                                                                               (arg-from
                                                                                                item 'index)
                                                                                               slot))
                                                                                            items)))))
                                                                                 groups)
                                                                                (let ((slots
                                                                                       (let loop
                                                                                           ((index 0)
                                                                                            (out '()))
                                                                                         (if (= index
                                                                                                (length paths))
                                                                                             (reverse out)
                                                                                             (loop
                                                                                              (+ index 1)
                                                                                              (cons
                                                                                               (vector-ref
                                                                                                proof-slots
                                                                                                index)
                                                                                               out))))))
                                                                                  (if remote?
                                                                                      (failure
                                                                                       (self-call
                                                                                        `((proofs ,proof-groups)
                                                                                          (slots ,slots))))
                                                                                      ((ledger
                                                                                        'pin-batch!)
                                                                                       paths
                                                                                       slots)))))))
                                                                       ((unpin-batch!)
                                                                        (let ((paths (arg 'paths)))
                                                                          (authorize-batch paths 'unpin!)
                                                                          ((ledger 'unpin-batch!) paths)))
                                                                       ((trace-batch)
                                                                        (let* ((index (or (arg 'index) -1))
                                                                               (paths
                                                                                (map
                                                                                 (lambda (path)
                                                                                   (if (and (pair? path)
                                                                                            (integer?
                                                                                             (car path)))
                                                                                       path
                                                                                       (cons index path)))
                                                                                 (arg 'paths)))
                                                                               (auth
                                                                                (authorize-batch
                                                                                 paths 'trace))
                                                                               (ancestors (cadr auth)))
                                                                          (if (member #t ancestors)
                                                                              (cadr
                                                                               (assoc
                                                                                'proof
                                                                                ((ledger
                                                                                  'retrieve-batch)
                                                                                 paths #f
                                                                                 (and (arg 'head)
                                                                                      (map
                                                                                       (lambda (path)
                                                                                         (arg 'head))
                                                                                       paths))
                                                                                 ancestors #t)))
                                                                              ((ledger 'trace-batch)
                                                                               paths (arg 'head)))))
                                                                       (else
                                                                        (error 'api-error
                                                                               "Unknown batch function")))
								 (let* ((preauthenticated
                                                                         (and retained-provider?
                                                                              (authenticate)))
                                                                        (selected
                                                                         (and
                                                                          (memq function
                                                                                '(retrieve pin! unpin! prune!))
                                                                          ((if retained-provider?
                                                                               retained-path committed-path)
                                                                           (arg 'path)))))
								   (cond
								    ((and (eq? function 'retrieve) (not retained-provider?)
                                                                          (pair? (arg-from selected 'route)))
                                                                     (let ((result
                                                                            (committed-invoke
                                                                             'retrieve (authenticate)
                                                                             selected arguments
                                                                             (arg 'index?))))
                                                                       (if (arg 'index?)
                                                                           (indexed-result result selected)
                                                                           result)))
								    ((and (eq? function 'pin!) (pair? (arg-from selected 'route)))
								     (let ((principal (authorize)))
								       (if prepared ((ledger 'pin!) (arg 'path) prepared)
								           (let* ((retrieve-arguments
								                   (set-alist (set-alist arguments 'proof? #t) 'pinned? #f))
								                  (retrieved (committed-invoke 'retrieve principal selected
								                                              retrieve-arguments #t))
								                  (proof (and (list? retrieved) (assoc 'proof retrieved)
								                              (cadr (assoc 'proof retrieved))))
								                  (proof-index
								                   (and (list? retrieved) (assoc 'proof-index retrieved)
								                        (cadr (assoc 'proof-index retrieved)))))
								             (if (not (and proof (integer? proof-index)))
								                 (error 'integrity-error "Federated retrieve did not return indexed proof"))
								             (failure
								              (self-call `((proof ,proof) (index ,proof-index))))))))
								    ((and (eq? function 'unpin!) (pair? (arg-from selected 'route)))
								     (authorize)
								     ((ledger 'unpin!) (arg 'path)))
								    (else
                                                                     (authorize preauthenticated)
								     (case function ((put!)
                                                                                         (let* ((object? (not (not (arg 'object?))))
                                                                                                (expected
                                                                                                 (assoc 'expected arguments)))
                                                                                           ((ledger 'put!)
                                                                                            (arg 'path)
                                                                                            (if object?
                                                                                                (arg 'value)
                                                                                                (encode (arg 'value)
                                                                                                        (arg 'expression?)))
                                                                                            object?
                                                                                            (if expected #t #f)
                                                                                            (and expected
                                                                                                 (if object?
                                                                                                     (cadr expected)
                                                                                                     (encode
                                                                                                      (cadr expected)
                                                                                                      (arg 'expression?)))))))
                                                                                        ((use!)
                                                                                         (let* ((method (arg 'method))
                                                                                                (use-arguments
                                                                                                 (or (arg 'arguments) '()))
                                                                                                (value
                                                                                                 ((ledger 'use!)
                                                                                                  (arg 'path) method
                                                                                                  use-arguments
                                                                                                  (not (not (arg 'read-only?))))))
                                                                                           (if (and (not method)
                                                                                                    (null? use-arguments)
                                                                                                    (not (and (list? value)
                                                                                                              (pair? value)
                                                                                                              (pair? (car value))
                                                                                                              (assoc 'class value)
                                                                                                              (assoc 'object-hash value)
                                                                                                              (assoc 'code-hash value))))
                                                                                               (stage-directory
                                                                                                (decode value (arg 'expression?)))
                                                                                               (resource-result
                                                                                                (decode value (arg 'expression?))
                                                                                                (arg 'expression?)))))
                                                                                        ((copy!)
                                                                                         (let ((expected
                                                                                                (assoc 'expected arguments)))
                                                                                           ((ledger 'copy!)
                                                                                            (arg 'source)
                                                                                            (arg 'path)
                                                                                            (if expected #t #f)
                                                                                            (and expected
                                                                                                 (encode
                                                                                                  (cadr expected)
                                                                                                  (arg 'expression?))))))
                                                                                        ((copy-batch!)
                                                                                         (let* ((sources (arg 'sources))
                                                                                                (paths (arg 'paths))
                                                                                                (expected-entry
                                                                                                 (assoc 'expected arguments))
                                                                                                (expected
                                                                                                 (and expected-entry
                                                                                                      (cadr expected-entry))))
                                                                                           (if (not
                                                                                                (and (list? sources)
                                                                                                     (list? paths)
                                                                                                     (= (length sources)
                                                                                                        (length paths))
                                                                                                     (or (not expected-entry)
                                                                                                         (and (list? expected)
                                                                                                              (= (length paths)
                                                                                                                 (length expected))))))
                                                                                               (error
                                                                                                'argument-error
                                                                                                "Copy sources, paths, and expected values must have equal lengths"))
                                                                                           ((ledger 'copy-batch!)
                                                                                            sources paths
                                                                                            (if expected-entry #t #f)
                                                                                            (if expected-entry
                                                                                                (map
                                                                                                 (lambda (old)
                                                                                                   (encode old
                                                                                                           (arg 'expression?)))
                                                                                                 expected)
                                                                                                '()))))
											((put-batch!)
                                                                                         (let* ((paths (arg 'paths))
                                                                                                (values (arg 'values))
                                                                                                (expected-entry
                                                                                                 (assoc 'expected arguments))
                                                                                                (expected
                                                                                                 (and expected-entry
                                                                                                      (cadr expected-entry)))
                                                                                                (object-entry (arg 'object?))
                                                                                                (objects
                                                                                                 (cond ((list? object-entry) object-entry)
                                                                                                       ((boolean? object-entry)
                                                                                                        (make-list (length paths)
                                                                                                                   object-entry))
                                                                                                       ((not object-entry)
                                                                                                        (make-list (length paths) #f))
                                                                                                       (else #f))))
                                                                                           (if (not
                                                                                                (and (list? paths)
                                                                                                     (list? values)
                                                                                                     (list? objects)
                                                                                                     (= (length paths)
                                                                                                        (length values)
                                                                                                     )
                                                                                                     (= (length paths)
                                                                                                        (length objects))
                                                                                                     (not (member #f
                                                                                                                  (map boolean?
                                                                                                                       objects)))
                                                                                                     (or (not expected-entry)
                                                                                                         (and (list? expected)
                                                                                                              (= (length paths)
                                                                                                                 (length expected))))))
                                                                                               (error
                                                                                                'argument-error
                                                                                                "Batch paths, values, object flags, and expected values must have equal lengths"))
                                                                                           ((ledger 'put-batch!)
                                                                                            (if expected-entry
                                                                                                (map
                                                                                                 (lambda (path value object? old)
                                                                                                   (list path
                                                                                                         (if object? value
                                                                                                             (encode value
                                                                                                                     (arg 'expression?)))
                                                                                                         object?
                                                                                                         (if object? old
                                                                                                             (encode old
                                                                                                                     (arg 'expression?)))))
                                                                                                 paths values objects expected)
                                                                                                (map
                                                                                                 (lambda (path value object?)
                                                                                                   (list path
                                                                                                         (if object? value
                                                                                                             (encode value
                                                                                                                     (arg 'expression?)))
                                                                                                         object?))
                                                                                                 paths values objects)))))
                                                                                        ((use-batch!)
                                                                                         (let* ((paths (arg 'paths))
                                                                                                (methods (or (arg 'methods)
                                                                                                             (make-list (length paths) #f)))
                                                                                                (batch-arguments
                                                                                                 (or (arg 'arguments)
                                                                                                     (make-list (length paths) '()))))
                                                                                           (if (not (and (list? paths)
                                                                                                         (list? methods)
                                                                                                         (list? batch-arguments)
                                                                                                         (= (length paths) (length methods))
                                                                                                         (= (length paths)
                                                                                                            (length batch-arguments))))
                                                                                               (error 'argument-error
                                                                                                      "Use batch fields must have equal lengths"))
                                                                                           (map
                                                                                            (lambda (value method arguments)
                                                                                              (if (and (not method)
                                                                                                       (null? arguments)
                                                                                                       (not (and (list? value)
                                                                                                                 (pair? value)
                                                                                                                 (pair? (car value))
                                                                                                                 (assoc 'class value)
                                                                                                                 (assoc 'object-hash value)
                                                                                                                 (assoc 'code-hash value))))
                                                                                                  (decode value (arg 'expression?))
                                                                                                  (resource-result
                                                                                                   (decode value (arg 'expression?))
                                                                                                   (arg 'expression?))))
                                                                                            ((ledger 'use-batch!)
                                                                                             (map list paths methods
                                                                                                  batch-arguments)
                                                                                             (not (not (arg 'read-only?))))
                                                                                            methods batch-arguments)))
                                                                                         ((run!)
                                                                                          (call-program
                                                                                           (decode ((ledger 'use!)
                                                                                                    (arg 'path) #f '() #t) #t)))
                                                                                         ((truncate!)
                                                                                          ((ledger 'truncate!)
                                                                                           (arg 'index)))
                                                                                         ((prune!) ((ledger 'prune!) (arg 'path)))
                                                                                         ((prune-batch!)
                                                                                          (begin (committed-groups (arg 'paths))
                                                                                                 ((ledger 'prune-batch!) (arg 'paths))))
											((retrieve)
 (let* ((path (arg 'path))
        (method (arg 'method))
        (resource-arguments (or (arg 'arguments) '()))
        (head (or (arg 'head) auth-head))
        (attempt ((ledger 'retrieve) path #f #f head ancestor? method resource-arguments))
        (active? (or (assoc 'method arguments) (assoc 'arguments arguments)))
        (head
         (cond
          ((and (not head) (equal? attempt '(unknown)) active?)
           (let* ((index (if (and (pair? path) (integer? (car path)))
                             (car path) -1))
                  (serialization
                   ((ledger '~trace) index
                    (if (and (pair? path) (integer? (car path)))
                        (cdr path) path)
                    #f method resource-arguments #t))
                  (evidence ((standard 'deserialize) serialization)))
             `((index ,(if (< index 0) (+ ((ledger 'size)) index) index))
               (object ,evidence))))
          ((and (not head) (equal? attempt '(unknown)) (pair? path)
                (let ((segment (if (integer? (car path)) (cadr path) (car path))))
                  (or (eq? segment '*bridge*)
                      (and (symbol? segment)
                           (not (memq segment '(*state* *transition* *crypto*)))))))
           (hydrate path))
          (else head)))
        (result ((ledger 'retrieve) path (arg 'pinned?) (arg 'proof?) head
                 ancestor? method resource-arguments))
        (result (if (or method (pair? resource-arguments) (sync-node? attempt))
                    (resource-result result (arg 'expression?))
                    (decode result (arg 'expression?)))))
   (if (arg 'index?) (indexed-result result selected) result)))
((trace)
 (let* ((path (arg 'path))
        (index (or (arg 'index) -1))
        (trace-path (cons index path))
        (head (arg 'head)))
   (if head
       ((ledger 'trace) path head)
       (let* ((retrieve-path (if (and (pair? path) (integer? (car path)))
                                path (cons index path)))
              (attempt ((ledger 'retrieve) retrieve-path)))
         (if (equal? attempt '(unknown))
             ((ledger 'trace) path (hydrate path index))
             ((ledger 'trace) trace-path))))))
((pin!)
											 (if prepared ((ledger 'pin!) (arg 'path) prepared)
											     (let* ((path (arg 'path))
											            (attempt ((ledger 'retrieve) path)))
											       (if (not (equal? attempt '(unknown)))
											           ((ledger 'pin!) path #f)
											           (failure
											            (self-call
											             `((proof ,((ledger 'trace) path (hydrate path)))
											               (index ,(- ((ledger 'size)) 1)))))))))
                                          ((unpin!) ((ledger 'unpin!) (arg 'path)))
											((bridge!) (federation-call 'bridge! (list (arg 'name)
																   `((interface ,(arg 'interface))
																     (remote-name ,(arg 'remote-name))))))
											((delete-bridge!) ((federation 'delete-bridge!) ledger (arg 'name)))
											((authorizations) ((authorization 'authorizations) (arg 'user)))
											((authorize!) ((authorization 'authorize!)
												       `((user ,(arg 'user)) (rule ,(arg 'rule))
													 (latest-index ,(- ((ledger 'size)) 1)))))
											((deauthorize!) ((authorization 'deauthorize!)
													 `((user ,(arg 'user)) (rule ,(arg 'rule)))))
											((config) (config-view (or (arg 'path) '())))
											((update-config!) ((ledger 'update-config!)
													   `((path ,(arg 'path)) (value ,(arg 'value))))
											 (if (equal? (arg 'path) '(public bridge-accept))
											     ((federation 'update-config!) `((path (bridge-accept))
															     (value ,(arg 'value))))) #t)
											((*secret*)
                         (let* ((secret (arg 'secret))
                                (verifier (sync-hash (expression->byte-vector secret)))
                                (public-key
                                 (car (transient-key
                                       'sync-web/interface-signing-key/v1 secret))))
                           (if (equal? verifier ((root 'get) '(interface root-verifier)))
                               (error 'argument-error "Root and Interface credentials must differ"))
                           ((root 'set!) '(interface credential) secret)
                           ((root 'set!) '(interface secret) verifier)
                           ((federation 'update-config!)
                            `((path (public-key)) (value ,public-key)))
                           ((ledger 'update-config!)
                            `((path (public interface public-key))
                              (value ,public-key)))))
                                          ((*admins-get*)
                                           (admin-principals->map
                                            ((root 'get) '(interface admins))))
                                          ((*admins-set*)
                                           ((root 'set!) '(interface admins)
                                            (admin-map->principals (arg 'admins))))
											((*window-set*) ((ledger 'update-config!)
													 `((path (public window)) (value ,(arg 'value)))))
											(else (error 'api-error "Unknown Interface function")))))))))))
					      (if (not delegated?)
					          (begin
					            ((root 'set!) '(root object ledger) (ledger))
					            ((root 'set!) '(root object federation) (federation))
					            ((root 'set!) '(root object authorization) (authorization))))
					      result)))))

  (set-query `(lambda (root query)
               (let ((once ,query-once)) (once query))))

  (define step-once '(lambda (root secret query)
		       (let* ((module (eval ((root 'get) '(root class standard-module))))
                              (std-node ((root 'get) '(root object standard)))
                              (standard (module 'local ((root 'get) '(root class standard)) std-node))
                              (ledger (module 'local ((root 'get) '(root class ledger))
                                              ((root 'get) '(root object ledger))))
                              (federation (module 'local ((root 'get) '(root class federation))
                                                  ((root 'get) '(root object federation))))
                              (salt ((ledger 'config) '(public key-derivation-salt)))
                              (continuation-key
                               (crypto-generate
                                (expression->byte-vector
                                 (list 'sync-web/federation-continuation-signing-key/v1 salt
                                       (sync-hash (expression->byte-vector secret)))))))
                         (define (self query blocking?) (sync-call `(*step* ,secret ,query) blocking?))
			 (define (run method operands data query) (let* ((input (if data
										    (append (reverse (cdr (reverse operands)))
											    (list `((value ,(car (reverse operands))) (continuation ,data))))
										    operands))
                                                                         (input (append input (list continuation-key)))
									 (result (apply (federation method) (cons ledger input)))
									 (next (and (pair? result) (pair? (car result)) (assoc 'continuation result))))
								    (if next (self (append query (list (cadr next))) #t) result)))
                         (define (commit report?)
                           (let* ((before ((ledger 'size)))
                                  (keys (crypto-generate (expression->byte-vector
                                                         (list 'sync-web/journal-signing-key/v1 salt
                                                               (sync-hash (expression->byte-vector secret))))))
                                  (size ((ledger 'step!) `((unix-time ,(system-time-unix))
                                                           (public-key ,(car keys))
                                                           (secret-key ,(cdr keys))))))
                             (if report?
                                 (list size (> size before)
                                       (not (equal? ((ledger 'use!) '(*state* *periodic*) #f '() #t)
                                                    '(nothing))))
                                 size)))
                         (let* ((query (if (null? query) '(ledger-step) query))
                                (outer-step? (and (eq? (car query) 'ledger-step)
                                                  (null? (cdr query))))
                                (result (case (car query)
											  ((ledger-step) (if (and (pair? (cdr query)) (cadr query))
                                                                                     (commit (and (pair? (cddr query)) (caddr query)))
													     (let ((ops (cadr (assoc 'operations ((federation 'step!) ledger)))))
													       (for-each (lambda (op) (self op #f)) ops)
                                                                               (let* ((commit-result (self '(ledger-step #t #t) #t))
                                                                                      (size (car commit-result)))
                                                                                 (if (and (cadr commit-result)
                                                                                          (caddr commit-result))
                                                                                     (sync-call
                                                                                      `((function run!)
                                                                                        (arguments
                                                                                         ((path (*state* *periodic*))
                                                                                          (arguments (,(- size 1)))))
                                                                                        (authentication
                                                                                         ((credentials
                                                                                           ,((root 'get) '(interface credential))))))
                                                                                      #f))
                                                                                 size))))
											  ((bridge-synchronize!) (run 'bridge-synchronize! (list (cadr query))
														      (and (pair? (cddr query)) (caddr query))
														      `(bridge-synchronize! ,(cadr query))))
											  (else (error 'api-error "Unknown step operation")))))
                           (if (not outer-step?)
                               (begin
                                 ((root 'set!) '(root object ledger) (ledger))
                                 ((root 'set!) '(root object federation) (federation))))
			   result))))
  (set-step step-once)
  (if (not fresh?)
      (call
       `(lambda (root)
          ;; Replace every built-in class changed since the preceding release.
          ((root 'set!) '(root class standard-module) ',standard-source)
          ((root 'set!) '(root class standard) ',standard-class)
          ((root 'set!) '(root class chain) ',chain)
          ((root 'set!) '(root class tree) ',tree)
          ((root 'set!) '(root class ledger) ',ledger)
          ((root 'set!) '(root class federation) ',federation)
          ((root 'set!) '(root class authorization) ',authorization)
          #t)))
  "Installed interface")
